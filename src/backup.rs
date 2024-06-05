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
use futures::stream::FuturesUnordered;

use crate::{
    get_glacier_file,
    get_changed_files,
    get_missing_files,
    get_new_files,
    get_pending_delete_files,
    get_pending_upload_files,
    get_pending_update_files,
};

use futures::{
    stream::FuturesOrdered,
    StreamExt,
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

    let failures: usize = complete_upsert(args, conn, s3_client, dynamo_client, &mut pending_upload_files).await;

    (length - failures, failures)
}

pub async fn fix_pending_updates(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {
    let mut pending_update_files: Vec<GlacierFile> = get_pending_update_files(conn);
    let length = pending_update_files.len();

    if args.dry_run {
        return (length, 0)
    }

    let failures: usize = complete_upsert(args, conn, s3_client, dynamo_client, &mut pending_update_files).await;

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

    let failures: usize = complete_upsert(args, conn, s3_client, dynamo_client, &mut glacier_files).await;

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
    
    let failures: usize = complete_upsert(args, conn, s3_client, dynamo_client, &mut glacier_files).await;

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

#[derive(Clone)]
struct HashJob {
    hash_tracker: HashTracker,
    file: GlacierFile,
}

async fn complete_upsert(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {

    let mut failures = 0;

    // Build S3 upsert jobs for every file to be upserted.

    // File will be uploaded for the first time
    let mut s3_upload = FuturesOrdered::new();
    let mut s3_upload_post = Vec::new();

    // File was previously uploaded and deleted, but has expired
    let mut s3_update = FuturesOrdered::new();
    let mut s3_update_post = Vec::new();

    // File was previously uploaded and was deleted
    let mut s3_undelete = FuturesOrdered::new();
    let mut s3_undelete_post = Vec::new();

    // File was previously uploaded and is still active
    let mut no_upload_post = Vec::new();

    let mut hash_jobs: &mut Vec<HashJob>;

    for file in files {

        let new_hash = hash_file(Path::new(&file.file_path), HASH_ALGO);
        let hash_tracker_result = HashTracker::get(
            dynamo_client,
            args.dynamo_table.clone(),
            new_hash.clone()).await;

        match hash_tracker_result {
            // If there was an active version at one point
            Ok(mut hash_tracker) => {

                // If there is no active version for the hash
                if hash_tracker.file_names.len() == 0 {
                    
                    // If all inactive versions have expired
                    if hash_tracker.expiration < Utc::now() {
                        hash_jobs = &mut s3_update_post;
                        s3_update.push_back(s3::put(s3_client, 
                            args.bucket_name.clone(), 
                            file.file_path.clone(),
                            hash_tracker.hash.clone()
                        ));

                        let expiration = match Utc::now().checked_add_signed(Duration::days(args.min_storage_duration)) {
                            Some(e) => e,
                            None => DateTime::UNIX_EPOCH,
                        };

                        hash_tracker.expiration = expiration;
                    }

                    // If there are inactive versions that have not expired
                    else {
                        hash_jobs = &mut s3_undelete_post;
                        s3_undelete.push_back(s3::undelete(s3_client,
                            args.bucket_name.clone(),
                            hash_tracker.hash.clone()
                        ));
                    }
                }

                // If there are active versions
                else {
                    hash_jobs = &mut no_upload_post;
                }

                file.uploaded = Some(file.modified);

                (*hash_jobs).push(HashJob {
                    hash_tracker,
                    file: file.clone(),
                });
            },

            // If this is the first time this file has been uploaded
            Err(_) => {
                let expiration = match Utc::now().checked_add_signed(Duration::days(args.min_storage_duration)) {
                    Some(e) => e,
                    None => DateTime::UNIX_EPOCH,
                };

                let hash_tracker = HashTracker {
                    hash: new_hash,
                    file_names: vec![file.file_path.clone()],
                    expiration,
                };

                file.uploaded = Some(file.modified);

                s3_upload.push_back(s3::put(s3_client, 
                    args.bucket_name.clone(), 
                    file.file_path.clone(),
                    hash_tracker.hash.clone()
                ));

                s3_upload_post.push(HashJob {
                    hash_tracker,
                    file: file.clone(),
                });
            },
        };
    };


    // NO UPLOAD
    let hash_futures = FuturesUnordered::new();

    for hash_job in &mut no_upload_post {
        let h_job = hash_job.clone();

        hash_futures.push(hash_job.hash_tracker.move_filename_from(
            dynamo_client, 
            args.dynamo_table.clone(), 
            hash_job.file.file_path.clone(), 
            hash_job.file.file_hash.clone()
        ));
        
        hash_job.file.file_hash = Some(h_job.hash_tracker.hash);
        hash_job.file.update(conn);
    }

    let _: Vec<_> = hash_futures.collect().await;

    // UNDELETE
    let hash_futures = FuturesUnordered::new();
    let results: Vec<_> = s3_undelete.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_undelete_post {
        let result = &results[i];
        let h_job = hash_job.clone();
        i += 1;

        match result {
            Ok(_) => {
                hash_futures.push(hash_job.hash_tracker.move_filename_from(
                    dynamo_client, 
                    args.dynamo_table.clone(), 
                    hash_job.file.file_path.clone(), 
                    hash_job.file.file_hash.clone()
                ));

                hash_job.file.file_hash = Some(h_job.hash_tracker.hash);
                hash_job.file.update(conn);
            },
            Err(_) => {
                s3_upload_post.push(hash_job.clone());
                s3_upload.push_back(s3::put(s3_client, 
                    args.bucket_name.clone(), 
                    hash_job.file.file_path.clone(),
                    hash_job.hash_tracker.hash.clone()));
            }
        }
    };

    let _: Vec<_> = hash_futures.collect().await;

    // UPLOAD
    let hash_futures = FuturesUnordered::new();
    let results: Vec<_> = s3_upload.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_upload_post {
        let result = &results[i];
        let h_job = hash_job.clone();
        i += 1;

        match result {
            Ok(_) => {
                hash_futures.push(hash_job.hash_tracker.move_filename_from(
                    dynamo_client, 
                    args.dynamo_table.clone(), 
                    hash_job.file.file_path.clone(), 
                    hash_job.file.file_hash.clone()
                ));

                hash_job.file.file_hash = Some(h_job.hash_tracker.hash);
                hash_job.file.update(conn);
            },
            Err(_) => {
                failures += 1;
            }
        }
    };

    let _: Vec<_> = hash_futures.collect().await;

    // UPDATE
    let hash_futures = FuturesUnordered::new();
    let results: Vec<_> = s3_update.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_update_post {
        let result = &results[i];
        let h_job = hash_job.clone();
        i += 1;

        match result {
            Ok(_) => {
                hash_futures.push(hash_job.hash_tracker.move_filename_from(
                    dynamo_client, 
                    args.dynamo_table.clone(), 
                    hash_job.file.file_path.clone(), 
                    hash_job.file.file_hash.clone()
                ));

                hash_job.file.file_hash = Some(h_job.hash_tracker.hash);
                hash_job.file.update(conn);
            },
            Err(_) => {
                failures += 1;
            }
        }
    };

    let _: Vec<_> = hash_futures.collect().await;

    failures
}

async fn complete_delete(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {

    let mut failures = 0;

    // Build S3 delete jobs for every file to be deleted.
    let mut s3_delete = FuturesOrdered::new();
    let mut s3_delete_post = Vec::new();

    let mut s3_delete_error = FuturesOrdered::new();
    let mut s3_delete_error_files = Vec::new();

    for file in files {

        let Some(hash) = file.file_hash.clone() else { continue; };

        let hash_tracker_result = HashTracker::get(dynamo_client, args.dynamo_table.clone(), hash.clone()).await;

        match hash_tracker_result {
            Ok(mut hash_tracker) => {
                let index_result = hash_tracker.file_names.iter().position(|x| *x == file.file_path.clone());

                match index_result {
                    Some(index) => {
                        hash_tracker.file_names.remove(index);

                        s3_delete.push_back(s3::delete(
                            s3_client,
                            args.bucket_name.clone(),
                            hash.clone()
                        ));

                        s3_delete_post.push(HashJob {
                            hash_tracker,
                            file: file.clone(),
                        });
                    },

                    None => {
                        s3_delete_error_files.push(file);
                        s3_delete_error.push_back(s3::delete(
                            s3_client,
                            args.bucket_name.clone(),
                            hash.clone()
                        ));
                    },
                };

            },
            Err(_) => {
                s3_delete_error_files.push(file);
                s3_delete_error.push_back(s3::delete(
                    s3_client,
                    args.bucket_name.clone(),
                    hash.clone()
                ));
            },
        };
    };


    // DELETE NORMALLY
    let hash_futures_delete = FuturesUnordered::new();
    let hash_futures_update = FuturesUnordered::new();
    let results: Vec<_> = s3_delete.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_delete_post {
        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {

                // If at least one version of the file will exist after deletion, update the dynamo entry
                if hash_job.hash_tracker.file_names.len() == 0 && hash_job.hash_tracker.expiration < Utc::now() {
                    hash_futures_delete.push(hash_job.hash_tracker.delete(
                        dynamo_client,
                        args.dynamo_table.clone()
                    ));
                }

                // If no versions of the file will exist after deletion, delete the entry from dynamo
                else {
                    hash_futures_update.push(hash_job.hash_tracker.put(
                        dynamo_client,
                        args.dynamo_table.clone()
                    ));
                };

                hash_job.file.delete(conn);
            },
            Err(_) => {
                failures += 1;
            }
        }
    };

    let _: Vec<_> = hash_futures_delete.collect().await;
    let _: Vec<_> = hash_futures_update.collect().await;

    // DELETE WITH DB ERRORS
    let length = s3_delete_error.len();
    let results: Vec<_> = s3_delete_error.collect().await;

    // Modify the database according to the results
    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Delete from glacier_state
                s3_delete_error_files[i].delete(conn);
            }
        }
    };

    failures
}