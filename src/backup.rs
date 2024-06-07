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

async fn complete_put<'a>(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {
    let mut failures = 0;

    for file in files {

        let new_hash = hash_file(Path::new(&file.file_path), HASH_ALGO);
        let result = HashTracker::get(
            dynamo_client,
            args.dynamo_table.clone(),
            new_hash.clone()
        ).await;

        let mut hash_tracker = match result {
            // If there was an active version at one point
            Ok(mut h_t) => {

                // If there are active versions
                if h_t.has_files() {
                    
                    match file.file_hash.clone() {

                        // If this file was changed locally
                        Some(file_hash) => {
                            
                            let result = HashTracker::get(dynamo_client, args.dynamo_table.clone(), file_hash.clone()).await;
                            let mut old_hash_tracker = match result {
                                Ok(value) => value,
                                Err(_) => {
                                    failures += 1;
                                    continue;
                                },
                            };

                            old_hash_tracker.del_file_name(file.file_path.clone());

                            if !old_hash_tracker.has_files() {
                                dbg!("No versions left. Deleting...");
                                let result = s3::delete(s3_client, args.bucket_name.clone(), file_hash.clone()).await;

                                if result.is_ok() && old_hash_tracker.expiration < Utc::now() {
                                    let result = old_hash_tracker.delete(dynamo_client, args.dynamo_table.clone()).await;

                                    if result.is_err() {
                                        dbg!(result.err());
                                        failures += 1;
                                        continue;
                                    }
                                };
                            };
                        },

                        // If this is a new file locally
                        _ => (),
                    };
                }
                
                // If there is no active version for the hash
                else {
                    
                    // If all inactive versions have expired
                    if h_t.expiration < Utc::now() {

                        let result = s3::put(s3_client, args.bucket_name.clone(), file.file_path.clone(),h_t.hash.clone()).await;
                        dbg!("All inactive versions have expired. Reuploading...");

                        if result.is_err() {
                            dbg!(result.err());
                            failures += 1;
                            continue;
                        };

                        h_t.expiration = new_expiration(args.min_storage_duration);
                    }

                    // If there are inactive versions that have not expired
                    else {
                        let result = s3::undelete(s3_client, args.bucket_name.clone(), h_t.hash.clone()).await;
                        dbg!("File is inactive, but has not expired. Undeleting...");

                        if result.is_err() {
                            dbg!(result.err());
                            failures += 1;
                            continue;
                        };
                    };
                };

                h_t
            },

            // If this is the first time this file has been uploaded
            Err(_) => {
                let result = s3::put(s3_client, args.bucket_name.clone(), file.file_path.clone(),new_hash.clone()).await;
                dbg!("New file. Uploading...");

                if result.is_err() {
                    dbg!(result.err());
                    failures += 1;
                    continue;
                };

                HashTracker::new (
                    new_hash.to_string(), 
                    new_expiration(args.min_storage_duration), 
                    file.file_path.clone()
                )
            },
        };

        hash_tracker.add_file_name(file.file_path.clone());

        // Update local database
        file.file_hash = Some(hash_tracker.hash.clone());
        file.uploaded = Some(file.modified);

        file.update(conn);

        let result = hash_tracker.put(dynamo_client, args.dynamo_table.clone()).await;

        if result.is_err() {
            dbg!(result.err());
            failures += 1;
            continue;
        };
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
            Ok(mut hash_tracker) => {

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
            Err(_) => {
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