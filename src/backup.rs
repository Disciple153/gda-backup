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

#[derive(Clone)]
struct HashJob {
    hash_tracker: HashTracker,
    file: GlacierFile,
}

#[derive(Clone)]
struct NoUploadLocalUpdateJob1 {
    old_hash: String,
    //old_hash_tracker: Option<HashTracker>,
    file: GlacierFile,
}

#[derive(Clone)]
struct NoUploadLocalUpdateJob2 {
    //old_hash: String,
    old_hash_tracker: HashTracker,
    file: GlacierFile,
}

async fn complete_put<'a>(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {
    let mut next_jobs_1 = Vec::new();

    let mut failures = 0;

    let mut get_hash_trackers = FuturesOrdered::new();
    let mut new_hashes = Vec::new();

    // COMPUTE HASHES
    for file in &mut *files {

        let new_hash = hash_file(Path::new(&file.file_path), HASH_ALGO);
        // TODO Batch this.
        get_hash_trackers.push_back( HashTracker::get(
            dynamo_client,
            args.dynamo_table.clone(),
            new_hash.clone()
        ));

        new_hashes.push(new_hash);
    };
    
    // GET HASH_TRACKERS
    let results: Vec<_> = get_hash_trackers.collect().await;
    let mut i = 0;

    // DECLARE JOB ORGANIZERS

    // File will be uploaded for the first time
    let mut s3_upload = FuturesOrdered::new();
    let mut s3_upload_post = Vec::new();

    // File was previously uploaded and deleted, but has expired
    let mut s3_reupload = FuturesOrdered::new();
    let mut s3_reupload_post = Vec::new();

    // File was previously uploaded and was deleted
    let mut s3_undelete = FuturesOrdered::new();
    let mut s3_undelete_post = Vec::new();

    
    // No upload local update
    let mut no_upload_local_upds = FuturesOrdered::new();
    let mut no_upload_local_jobs = Vec::new();

    // No upload local new
    let mut no_upload_local_news = FuturesOrdered::new();
    let mut no_upload_local_news_file = Vec::new();

    let mut hash_jobs: &mut Vec<HashJob>;

    // SORT FILES INTO JOB ORGANIZERS
    for file in files {
        let new_hash = &new_hashes[i];
        let result = &results[i];
        i += 1;

        match result {
            // If there was an active version at one point
            Ok(hash_tracker) => {

                let mut h_t = hash_tracker.clone();

                // If there are active versions
                if h_t.has_files() {
                    match file.file_hash.clone() {

                        // If this file was changed locally
                        Some(file_hash) => {

                            // Add filename to new dynamodb hash entry
                            h_t.add_file_name(file.file_path.clone());
                            no_upload_local_upds.push_back(hash_tracker.put(dynamo_client, args.dynamo_table.clone()));

                            // Update local database
                            file.file_hash = Some(h_t.hash.clone());
                            file.uploaded = Some(file.modified);

                            no_upload_local_jobs.push(NoUploadLocalUpdateJob1 {
                                old_hash: file_hash,
                                file: file.clone(),
                            });
                        },

                        // If this is a new file locally
                        None => {
                            
                            // Add filename to new dynamodb hash entry
                            h_t.add_file_name(file.file_path.clone());
                            no_upload_local_news.push_back(hash_tracker.put(dynamo_client, args.dynamo_table.clone()));

                            // Update local database
                            file.file_hash = Some(h_t.hash.clone());
                            file.uploaded = Some(file.modified);
                            no_upload_local_news_file.push(file);
                        },
                    };
                }
                
                // If there is no active version for the hash
                else {
                    
                    // If all inactive versions have expired
                    if h_t.expiration < Utc::now() {
                        hash_jobs = &mut s3_reupload_post;
                        s3_reupload.push_back(s3::put(s3_client, 
                            args.bucket_name.clone(), 
                            file.file_path.clone(),
                            h_t.hash.clone()
                        ));

                        let expiration = match Utc::now().checked_add_signed(Duration::days(args.min_storage_duration)) {
                            Some(e) => e,
                            None => DateTime::UNIX_EPOCH,
                        };

                        h_t.expiration = expiration;
                    }

                    // If there are inactive versions that have not expired
                    else {
                        hash_jobs = &mut s3_undelete_post;
                        s3_undelete.push_back(s3::undelete(s3_client,
                            args.bucket_name.clone(),
                            h_t.hash.clone()
                        ));
                    };

                    h_t.add_file_name(file.file_path.clone());
                    file.file_hash = Some(hash_tracker.hash.clone());
                    file.uploaded = Some(file.modified);

                    (*hash_jobs).push(HashJob {
                        hash_tracker: h_t.clone(),
                        file: file.clone(),
                    });
                };
            },

            // If this is the first time this file has been uploaded
            Err(_) => {
                let expiration = match Utc::now().checked_add_signed(Duration::days(args.min_storage_duration)) {
                    Some(e) => e,
                    None => DateTime::UNIX_EPOCH,
                };

                let mut hash_tracker = HashTracker::new (new_hash.to_string(), expiration, file.file_path.clone());
                
                hash_tracker.add_file_name(file.file_path.clone());
                file.file_hash = Some(hash_tracker.hash.clone());
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

    // NO UPLOAD LOCAL UPDATE

    // update new hash trackers
    let results: Vec<_> = no_upload_local_upds.collect().await;
    let mut next_futures = FuturesOrdered::new();
    let mut next_jobs = Vec::new();
    let mut i = 0;

    for job in no_upload_local_jobs {
        let result = &results[i];
        i += 1;

        dbg!("No upload local update");

        match result {
            Ok(_) => {
                next_futures.push_back(HashTracker::get(dynamo_client, args.dynamo_table.clone(), job.old_hash.clone()));
                next_jobs.push(job);
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            },
        };
    }
    
    // get old hash trackers
    let results: Vec<_> = next_futures.collect().await;
    let jobs = next_jobs;
    let mut next_futures = FuturesOrdered::new();
    let mut i = 0;

    let mut local_files_to_update = Vec::new();

    for job in jobs {
        let result = &results[i];
        i += 1;

        match result {
            Ok(hash_tracker) => {
                let mut old_hash_tracker = hash_tracker.clone();
                old_hash_tracker.del_file_name(job.file.file_path.clone());

                if old_hash_tracker.has_files() {
                    local_files_to_update.push(job.file);
                }
                else {
                    next_futures.push_back(s3::delete(s3_client, args.bucket_name.clone(), job.old_hash.clone()));
                    next_jobs_1.push(NoUploadLocalUpdateJob2 {
                        old_hash_tracker,
                        file: job.file,
                    });
                };
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            },
        };
    }

    // remove file from old hash tracker
    // if old hash tracker is emptied
        // delete file from s3
    let results: Vec<_> = next_futures.collect().await;
    let mut next_futures = FuturesOrdered::new();
    let mut next_jobs = Vec::new();
    let mut i = 0;

    for job in &mut next_jobs_1 {
        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                if job.old_hash_tracker.expiration < Utc::now() {
                    next_jobs.push(job.clone());
                    next_futures.push_back(job.old_hash_tracker.delete(dynamo_client, args.dynamo_table.clone()));
                }
                else {
                    local_files_to_update.push(job.file.clone());
                };
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            },
        };
    }

        // if old hash tracker is expired
            // delete old hash tracker
    let results: Vec<_> = next_futures.collect().await;
    let jobs = next_jobs;
    let mut i = 0;

    for job in jobs {
        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                local_files_to_update.push(job.file.clone());
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            },
        };
    }

    // update local database entry
    for file in local_files_to_update {
        file.update(conn);
    };

    // NO UPLOAD LOCAL NEW
    let results: Vec<_> = no_upload_local_news.collect().await;
    let mut i = 0;

    for no_upload_local_new_file in no_upload_local_news_file {
        dbg!("File moved. Creating reference...");
        
        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                no_upload_local_new_file.update(conn);
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            },
        };

    };

    // UNDELETE
    let hash_futures = FuturesUnordered::new();
    let results: Vec<_> = s3_undelete.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_undelete_post {
        dbg!("File undeleted. Undeleting file...");

        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                hash_futures.push(hash_job.hash_tracker.put(dynamo_client, args.dynamo_table.clone()));
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
        dbg!("File created. Putting new file...");

        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                hash_futures.push(hash_job.hash_tracker.put(dynamo_client, args.dynamo_table.clone()));
                hash_job.file.update(conn);
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            }
        }
    };

    let _: Vec<_> = hash_futures.collect().await;

    // REUPLOAD
    let hash_futures = FuturesUnordered::new();
    let results: Vec<_> = s3_reupload.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_reupload_post {
        dbg!("File recreated. Putting new file...");

        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                hash_futures.push(hash_job.hash_tracker.put(dynamo_client, args.dynamo_table.clone()));
                hash_job.file.update(conn);
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            }
        }
    };

    let _: Vec<_> = hash_futures.collect().await;

    failures
}

async fn complete_delete(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient, files: &mut Vec<GlacierFile>) -> usize {

    let mut failures = 0;

    // File was deleted, and there are no references to the file's hash in DynamoDB
    let mut s3_delete = FuturesOrdered::new();
    let mut s3_delete_post = Vec::new();

    // File was deleted, but there was an error when getting the file's hash in DynamoDB
    let mut s3_delete_error = FuturesOrdered::new();
    let mut s3_delete_error_files = Vec::new();

    // File was deleted, but there is still at least one reference to the file's hash in DynamoDB
    let mut no_delete_post = Vec::new();

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
                    no_delete_post.push(HashJob {
                        hash_tracker,
                        file: file.clone(),
                    });
                }

                // If the file was the only file referenced by the DynamoDB entry
                else {
                    s3_delete.push_back(s3::delete(
                        s3_client,
                        args.bucket_name.clone(),
                        hash.clone()
                    ));

                    s3_delete_post.push(HashJob {
                        hash_tracker,
                        file: file.clone(),
                    });
                };
            },

            // If the file's hash was not found in DynamoDB
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

    // NO DELETE
    let hash_futures = FuturesUnordered::new();

    for hash_job in &mut no_delete_post {
        dbg!("File moved. Deleting reference...");

        hash_futures.push(hash_job.hash_tracker.put(dynamo_client, args.dynamo_table.clone()));
        hash_job.file.delete(conn);
    }

    let _: Vec<_> = hash_futures.collect().await;

    // DELETE NORMALLY
    let hash_futures_delete = FuturesUnordered::new();
    let hash_futures_update = FuturesUnordered::new();
    let results: Vec<_> = s3_delete.collect().await;
    let mut i = 0;

    for hash_job in &mut s3_delete_post {
        dbg!("File deleted. Putting delete marker...");

        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {

                // If at least one version of the file will exist after deletion, update the dynamo entry
                if !hash_job.hash_tracker.has_files() && hash_job.hash_tracker.expiration < Utc::now() {
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
            Err(error) => {
                dbg!(error);
                failures += 1;
            }
        }
    };

    let _: Vec<_> = hash_futures_delete.collect().await;
    let _: Vec<_> = hash_futures_update.collect().await;

    // DELETE WITH DB ERRORS
    let results: Vec<_> = s3_delete_error.collect().await;
    let mut i = 0;

    // Modify the database according to the results
    for file in s3_delete_error_files {
        dbg!("File deleted with database error. Removing from local database...");

        let result = &results[i];
        i += 1;

        match result {
            Ok(_) => {
                // Delete from glacier_state
                file.delete(conn);
            },
            Err(error) => {
                dbg!(error);
                failures += 1;
            }
        }
    };

    failures
}