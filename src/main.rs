use std::{
    io::Error, 
    time::SystemTime
};
use aws_sdk_s3::Client as S3Client;
use walkdir::WalkDir;
use diesel::prelude::*;
use checksums::hash_file;

// Use BLAKE2B if running on 64 bit CPU
#[cfg(target_pointer_width = "64")]
use checksums::Algorithm::BLAKE2B as HASH_ALGO;

// Use BLAKE2S if running on 32 bit CPU or lower
#[cfg(not(target_pointer_width = "64"))]
use checksums::Algorithm::BLAKE2S as HASH_ALGO;

use glacier_sync::{
    clear_local_state,
    establish_connection,
    get_changed_files,
    get_missing_files,
    get_new_files,
    get_pending_delete_files,
    get_pending_upload_files,
    get_pending_update_files,
    glacier_state_is_empty,
    models::*
};

use futures::{
    stream::FuturesOrdered,
    StreamExt
};

mod s3;

const BUCKET_NAME: &str = "disciple153-test";
const DRY_RUN: bool = true;
const TARGET_DIR: &str = "/home/disciple153/documents/gda-backup/backup_test";

#[tokio::main]
async fn main() -> Result<(), Error> {

    // VARIABLES
    let mut successful_uploads: usize = 0;
    let mut successful_updates: usize = 0;
    let mut successful_deletes: usize = 0;
    let mut failed_uploads: usize = 0;
    let mut failed_updates: usize = 0;
    let mut failed_deletes: usize = 0;

    // GET CONNECTIONS
    let conn: &mut PgConnection = &mut establish_connection();
    let s3_client: &mut S3Client = &mut s3::get_client().await;

    // LOAD STATE 

    // Clear local_state from database
    clear_local_state(conn);

    // Load local_state into database
    for file in WalkDir::new(TARGET_DIR).into_iter().filter_map(|e: Result<walkdir::DirEntry, walkdir::Error>| e.ok()) {
        if file.metadata()?.is_file() {
            LocalFile {
                file_path: file.path().display().to_string(),
                modified: file.metadata()?.modified()?
            }.insert(conn);
        }
    }

    // If glacier_state is empty, populate it from Glacier.
    if glacier_state_is_empty(conn) {
        println!("Glacier state empty. Loading state from S3");
        load_from_s3(conn, &s3_client).await;
    }

    // UPLOAD NEW FILES

    // Upload all new files
    let (successes, failures) = upload_new_files(conn, s3_client).await;
    successful_uploads += successes;
    failed_uploads += failures;
    
    // Update all changed files
    let (successes, failures) = update_changed_files(conn, s3_client).await;
    successful_updates += successes;
    failed_updates += failures;
    
    // Add delete markers to missing files
    let (successes, failures) = delete_missing_files(conn, s3_client).await;
    successful_deletes += successes;
    failed_deletes += failures;
    
    // FIX PENDING BACKUPS

    // Upload all files in glacier state with null uploaded rows
    let (successes, failures) = fix_pending_uploads(conn, s3_client).await;
    successful_uploads += successes;
    failed_uploads += failures;

    // Upsert all files in glacier state with mismatched modified and uploaded rows
    let (successes, failures) = fix_pending_updates(conn, s3_client).await;
    successful_updates += successes;
    failed_updates += failures;
    
    // Delete all files in glacier pending deletion
    let (successes, failures) = fix_pending_deletes(conn, s3_client).await;
    successful_deletes += successes;
    failed_deletes += failures;
    
    // CLEAR STATE 
    clear_local_state(conn);

    // PRINT RESULTS
    println!("Backup complete:
Uploads: {successful_uploads} succeeded, {failed_uploads} failed.
Updates: {successful_updates} succeeded, {failed_updates} failed.
Deletes: {successful_deletes} succeeded, {failed_deletes} failed.");

    Ok(())
}

async fn load_from_s3(conn: &mut PgConnection, s3_client: &S3Client) {
    let mut s3_paginator = s3::list(&s3_client, BUCKET_NAME).send();

    if DRY_RUN {
        return ()
    }

    while let Some(result) = s3_paginator.next().await {
        match result {
            Ok(output) => {
                for object in output.contents() {
                    let last_modified: SystemTime = SystemTime::try_from(*object.last_modified().unwrap()).expect("msg");

                    GlacierFile {
                        file_path: object.key().unwrap_or("Unknown").to_string(),
                        modified: last_modified,
                        uploaded: Some(last_modified),
                        pending_delete: false,
                    }.insert(conn);
                }
            }
            Err(err) => {
                eprintln!("{err:?}")
            }
        }
    }
}

async fn fix_pending_uploads(conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut failures: usize = 0;
    let pending_upload_files: Vec<GlacierFile> = get_pending_upload_files(conn);
    let length = pending_upload_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    for file in pending_upload_files {
        futures.push_back(s3::upsert(s3_client, BUCKET_NAME, file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };

    let results: Vec<_> = futures.collect().await;

    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                files[i].uploaded = Some(files[i].modified);
                files[i].update(conn);
            }
        }
    };

    (length - failures, failures)
}

async fn fix_pending_updates(conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut failures: usize = 0;
    let pending_update_files: Vec<GlacierFile> = get_pending_update_files(conn);
    let length = pending_update_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    for file in pending_update_files {
        futures.push_back(s3::upsert(s3_client, BUCKET_NAME, file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };

    let results: Vec<_> = futures.collect().await;

    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                files[i].uploaded = Some(files[i].modified);
                files[i].update(conn);
            }
        }
    };

    (length - failures, failures)
}

async fn fix_pending_deletes(conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut failures: usize = 0;
    let pending_delete_files: Vec<GlacierFile> = get_pending_delete_files(conn);
    let length = pending_delete_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    for file in pending_delete_files {
        futures.push_back(s3::delete(s3_client, BUCKET_NAME, file.file_path.clone()));
        files.push(file);
    };
    
    let results: Vec<_> = futures.collect().await;

    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                files[i].delete(conn);
            }
        }
    };

    (length - failures, failures)
}

async fn upload_new_files(conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut failures: usize = 0;
    let new_files: Vec<LocalFile> = get_new_files(conn);
    let length = new_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);


    for file in new_files {
        // Copy from local_state to glacier state, leaving uploaded null.
        let file = GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.insert(conn);

        // Upload to glacier.
        futures.push_back(s3::upsert(s3_client, BUCKET_NAME, file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };

    let results: Vec<_> = futures.collect().await;

    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                files[i].uploaded = Some(files[i].modified);
                files[i].update(conn);
            }
        }
    };

    (length - failures, failures)
}

async fn update_changed_files(conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut failures: usize = 0;
    let updated_files: Vec<LocalFile> = get_changed_files(conn);
    let length = updated_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    for file in updated_files {
        // Copy from local_state to glacier state, leaving uploaded as it was.
        let file = GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.update(conn);

        futures.push_back(s3::upsert(s3_client, BUCKET_NAME, file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };
    
    let results: Vec<_> = futures.collect().await;

    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                files[i].uploaded = Some(files[i].modified);
                files[i].update(conn);
            }
        }
    };

    (length - failures, failures)
}

async fn delete_missing_files(conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut failures: usize = 0;
    let deleted_files: Vec<GlacierFile> = get_missing_files(conn);
    let length = deleted_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    for mut file in deleted_files {
        // Set pending_delete to TRUE.
        file.pending_delete = true;
        file.update(conn);

        futures.push_back(s3::delete(s3_client, BUCKET_NAME, file.file_path.clone()));
        files.push(file);
    };
    
    let results: Vec<_> = futures.collect().await;

    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Delete from glacier_state
                files[i].delete(conn);
            }
        }
    };

    (length - failures, failures)
}

