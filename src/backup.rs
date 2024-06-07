use std::path::Path;

use crate::dynamodb::HashTracker;
use crate::environment::Args;
use crate::models::{
    GlacierFile,
    LocalFile,
};

use crate::s3;

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use chrono::{
    DateTime,
    Duration,
    Utc,
};
use diesel::prelude::PgConnection;

use crate::{
    get_glacier_file,
    get_changed_files,
    get_missing_files,
    get_new_files,
    get_pending_delete_files,
    get_pending_upload_files,
    get_pending_update_files,
};

use checksums::hash_file;

// Use BLAKE2B if running on 64 bit CPU
#[cfg(target_pointer_width = "64")]
use checksums::Algorithm::BLAKE2B as HASH_ALGO;

// Use BLAKE2S if running on 32 bit CPU or lower
#[cfg(not(target_pointer_width = "64"))]
use checksums::Algorithm::BLAKE2S as HASH_ALGO;

pub async fn fix_pending_uploads(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let mut pending_upload_files: Vec<GlacierFile> = get_pending_upload_files(conn);
    let length = pending_upload_files.len();

    if args.dry_run {
        return (length, 0)
    }

    let failures: usize = complete_put(args, conn, s3_client, dynamo_client, &mut pending_upload_files).await;

    (length - failures, failures)
}

pub async fn fix_pending_updates(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let mut pending_update_files: Vec<GlacierFile> = get_pending_update_files(conn);
    let length = pending_update_files.len();

    if args.dry_run {
        return (length, 0)
    }

    let failures: usize = complete_put(args, conn, s3_client, dynamo_client, &mut pending_update_files).await;

    (length - failures, failures)
}

pub async fn fix_pending_deletes(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let mut pending_delete_files: Vec<GlacierFile> = get_pending_delete_files(conn);
    let length = pending_delete_files.len();

    if args.dry_run {
        return (length, 0)
    }
    
    let failures: usize = complete_delete(args, conn, s3_client, dynamo_client, &mut pending_delete_files).await;

    (length - failures, failures)
}

pub async fn upload_new_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let new_files: Vec<LocalFile> = get_new_files(conn);
    let length = new_files.len();
    let mut glacier_files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in new_files {
        // Copy from local_state to glacier state, leaving uploaded null.
        glacier_files.push(GlacierFile {
            file_path: file.file_path,
            file_hash: None,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.insert(conn));
    };

    let failures: usize = complete_put(args, conn, s3_client, dynamo_client, &mut glacier_files).await;

    (length - failures, failures)
}

pub async fn update_changed_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let updated_files: Vec<LocalFile> = get_changed_files(conn);
    let length = updated_files.len();
    let mut glacier_files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for local_file in updated_files {
        // Copy from local_state to glacier state, leaving uploaded as it was.
        match get_glacier_file(conn, local_file.file_path) {
            Ok(mut glacier_file) => {
                glacier_file.modified = local_file.modified;
                glacier_files.push(glacier_file);
            },
            Err(_) => (),
        };
    };
    
    let failures: usize = complete_put(args, conn, s3_client, dynamo_client, &mut glacier_files).await;

    (length - failures, failures)
}

pub async fn delete_missing_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let mut deleted_files: Vec<GlacierFile> = get_missing_files(conn);
    let length = deleted_files.len();

    if args.dry_run {
        return (length, 0)
    }

    for file in &mut *deleted_files {
        // Set pending_delete to TRUE.
        file.pending_delete = true;
        file.update(conn);
    };
    
    let failures: usize = complete_delete(args, conn, s3_client, dynamo_client, &mut deleted_files).await;

    (length - failures, failures)
}


fn new_expiration(min_storage_duration: i64) -> DateTime<Utc> {
    match Utc::now().checked_add_signed(Duration::days(min_storage_duration)) {
        Some(e) => e,
        None => DateTime::UNIX_EPOCH,
    }
}

async fn get_old_hash_tracker(args: &Args, s3_client: &S3Client, dynamo_client: &DynamoClient, file: GlacierFile) -> Option<HashTracker> {
    let mut hash_tracker = HashTracker::get(dynamo_client, args.dynamo_table.clone(), file.file_hash.clone()?).await?;
    hash_tracker.del_file_name(file.file_path);

    if !hash_tracker.has_files() {
        let _ = s3::delete(s3_client, args.bucket_name.clone(), file.file_hash?);
    };

    Some(hash_tracker)
}

async fn complete_put(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {
    let mut failures = 0;

    for file in files {

        // GET HASH
        let new_hash = hash_file(Path::new(&file.file_path), HASH_ALGO);

        // UPDATE DATABASE OBJECTS

        // Get old hash tracker from old file hash, and remove old file name.
        let old_hash_tracker = get_old_hash_tracker(args, s3_client, dynamo_client, file.clone()).await;

        // Get new hash tracker from new file hash, or create a new hash tracker.
        let mut new_hash_tracker = HashTracker::get(
            dynamo_client,
            args.dynamo_table.clone(),
            new_hash.clone()
        ).await.unwrap_or(HashTracker::new(new_hash.to_string()));

        // If there are no active versions and all inactive versions have expired
        if !new_hash_tracker.has_files() && new_hash_tracker.is_expired() {

            dbg!("All inactive versions have expired. Reuploading...");
            match s3::put(s3_client, args.bucket_name.clone(), file.file_path.clone(),new_hash_tracker.hash.clone()).await {
                Ok(_) => (),
                Err(error) => {
                    dbg!(error);
                    failures += 1;
                    continue;
                },
            };

            new_hash_tracker.expiration = new_expiration(args.min_storage_duration);
        }

        // If there are inactive versions that have not expired
        else if !new_hash_tracker.has_files() && !new_hash_tracker.is_expired() {

            dbg!("File is inactive, but has not expired. Undeleting...");
            match s3::undelete(s3_client, args.bucket_name.clone(), new_hash_tracker.hash.clone()).await {
                Ok(_) => (),
                Err(error) => {
                    dbg!(error);
                    failures += 1;
                    continue;
                },
            };
        };

        // Add file name to new hash tracker
        new_hash_tracker.add_file_name(file.file_path.clone());

        // Update local file object
        file.file_hash = Some(new_hash_tracker.hash.clone());
        file.uploaded = Some(file.modified);

        // UPLOAD DATABASE CHANGES

        // Upload/delete old hash tracker
        match old_hash_tracker {
            Some(o_h_t) => {
                if !o_h_t.has_files() && o_h_t.is_expired() {
                    match o_h_t.delete(dynamo_client, args.dynamo_table.clone()).await {
                        Ok(_) => (),
                        Err(error) => {
                            dbg!(error);
                            failures += 1;
                            continue;
                        }
                    };
                }
                else {
                    match o_h_t.put(dynamo_client, args.dynamo_table.clone()).await {
                        Ok(_) => (),
                        Err(error) => {
                            dbg!(error);
                            failures += 1;
                            continue;
                        }
                    };
                }
            }
            None => {}
        };

        // Upload new hash tracker
        match new_hash_tracker.put(dynamo_client, args.dynamo_table.clone()).await {
            Ok(_) => (),
            Err(error) => {
                dbg!(error);
                failures += 1;
                continue;
            },
        };

        // Upload local database
        file.update(conn);
    };

    failures
}

async fn complete_delete(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {

    let mut failures = 0;

    for file in files {

        let hash = match file.file_hash.clone() {
            Some(value) => value,
            None => {
                file.delete(conn);
                continue;
            },
        };

        let hash_tracker_result = HashTracker::get(dynamo_client, args.dynamo_table.clone(), hash.clone()).await;

        match hash_tracker_result {

            // If the file's hash was found in DynamoDB
            Some(mut hash_tracker) => {

                // Remove the file path from the hash_tracker
                hash_tracker.del_file_name(file.file_path.clone());

                // If there were other files referenced by the DynamoDB entry
                if hash_tracker.has_files() {
                    dbg!("Other local files reference this file. Updating DynamoDb...");

                    let result = hash_tracker.put(dynamo_client, args.dynamo_table.clone()).await;

                    if result.is_err() {
                        dbg!(result.err());
                        failures += 1;
                        continue;
                    };
                }

                // If the file was the only file referenced by the DynamoDB entry
                else {
                    let result = s3::delete(s3_client, args.bucket_name.clone(), hash.clone()).await;
                    dbg!("No more local files reference this file. Adding delete marker...");

                    if result.is_err() {
                        dbg!(result.err());
                        failures += 1;
                        continue;
                    };

                    // If at least one version of the file will exist after deletion, update the dynamo entry
                    if !hash_tracker.has_files() && hash_tracker.expiration < Utc::now() {
                        let result = hash_tracker.delete(dynamo_client, args.dynamo_table.clone()).await;

                        if result.is_err() {
                            dbg!(result.err());
                            failures += 1;
                            continue;
                        };
                    }

                    // If no versions of the file will exist after deletion, delete the entry from dynamo
                    else {
                        let result = hash_tracker.put(dynamo_client, args.dynamo_table.clone()).await;

                        if result.is_err() {
                            dbg!(result.err());
                            failures += 1;
                            continue;
                        };
                    };
                };
            },

            // If the file's hash was not found in DynamoDB
            None => {
                let result = s3::delete(s3_client, args.bucket_name.clone(), hash.clone()).await;
                dbg!("File not found in DynamoDB. Adding delete marker...");

                if result.is_err() {
                    dbg!(result.err());
                    failures += 1;
                    continue;
                };
            },
        };

        file.delete(conn);
    };

    failures
}