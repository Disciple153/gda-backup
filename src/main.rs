use std::io::Error;

use aws_sdk_s3::Client as S3Client;
use clap::Parser;
use walkdir::WalkDir;
use diesel::prelude::*;
use checksums::hash_file;

// Use BLAKE2B if running on 64 bit CPU
#[cfg(target_pointer_width = "64")]
use checksums::Algorithm::BLAKE2B as HASH_ALGO;

// Use BLAKE2S if running on 32 bit CPU or lower
#[cfg(not(target_pointer_width = "64"))]
use checksums::Algorithm::BLAKE2S as HASH_ALGO;

use glacier_sync::environment::Args;

use glacier_sync::{
    clear_local_state,
    establish_connection,
    glacier_state_is_empty,
    models::*
};

use glacier_sync::backup::{
    fix_pending_uploads,
    fix_pending_updates,
    fix_pending_deletes,
    upload_new_files,
    update_changed_files,
    delete_missing_files,
};

use glacier_sync::restore;
use glacier_sync::s3;

#[tokio::main]
async fn main() -> Result<(), Error> {

    // ARGUMENTS
    let args = &Args::parse();

    // VARIABLES
    let mut successful_uploads: usize = 0;
    let mut successful_updates: usize = 0;
    let mut successful_deletes: usize = 0;
    let mut failed_uploads: usize = 0;
    let mut failed_updates: usize = 0;
    let mut failed_deletes: usize = 0;

    // GET CONNECTIONS
    let conn: &mut PgConnection = &mut establish_connection(args);
    let s3_client: &mut S3Client = &mut s3::get_client().await;

    // LOAD STATE 

    // Clear local_state from database
    clear_local_state(conn);

    // Load local_state into database
    for file in WalkDir::new(args.target_dir.clone()).into_iter().filter_map(|e: Result<walkdir::DirEntry, walkdir::Error>| e.ok()) {
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
        restore::db_from_s3(args, conn, &s3_client).await;
    }

    // UPLOAD NEW FILES

    // Upload all new files
    let (successes, failures) = upload_new_files(args, conn, s3_client).await;
    successful_uploads += successes;
    failed_uploads += failures;
    
    // Update all changed files
    let (successes, failures) = update_changed_files(args, conn, s3_client).await;
    successful_updates += successes;
    failed_updates += failures;
    
    // Add delete markers to missing files
    let (successes, failures) = delete_missing_files(args, conn, s3_client).await;
    successful_deletes += successes;
    failed_deletes += failures;
    
    // FIX PENDING BACKUPS

    // Upload all files in glacier state with null uploaded rows
    let (successes, failures) = fix_pending_uploads(args, conn, s3_client).await;
    successful_uploads += successes;
    failed_uploads += failures;

    // Upsert all files in glacier state with mismatched modified and uploaded rows
    let (successes, failures) = fix_pending_updates(args, conn, s3_client).await;
    successful_updates += successes;
    failed_updates += failures;
    
    // Delete all files in glacier pending deletion
    let (successes, failures) = fix_pending_deletes(args, conn, s3_client).await;
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


