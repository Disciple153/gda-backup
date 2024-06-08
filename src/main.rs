use std::io::Error;

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use clap::Parser;
use walkdir::WalkDir;
use diesel::prelude::PgConnection;
use log::{LevelFilter, error};
use env_logger::Builder;

use glacier_sync::environment::Args;

use glacier_sync::{
    clear_local_state,
    establish_connection,
    glacier_state_is_empty,
    models::LocalFile
};

use glacier_sync::backup::backup;

use glacier_sync::restore;
use glacier_sync::s3;
use glacier_sync::dynamodb;

#[tokio::main]
async fn main() -> Result<(), Error> {

    // ARGUMENTS
    let args = &Args::parse();

    // SET LOG LEVEL

    if args.quiet {
        Builder::new().filter_level(LevelFilter::Error).init();
    }
    else if args.debug {
        Builder::new().filter_level(LevelFilter::Debug).init();
    }
    else {
        Builder::new().filter_level(LevelFilter::Info).init();
    }

    // GET CONNECTIONS
    let conn: &mut PgConnection = &mut establish_connection(args);
    let s3_client: &mut S3Client = &mut s3::get_client().await;
    let dynamo_client: &mut DynamoClient = &mut dynamodb::get_client().await;

    // LOAD STATE 

    // Clear local_state from database
    clear_local_state(conn);

    // Load local_state into database
    for file in WalkDir::new(args.target_dir.clone()).into_iter().filter_map(|e: Result<walkdir::DirEntry, walkdir::Error>| e.ok()) {
        if file.metadata()?.is_file() {
            let result = LocalFile {
                file_path: file.path().display().to_string(),
                modified: file.metadata()?.modified()?
            }.insert(conn);

            match result {
                Ok(_) => (),
                Err(error) => error!("Failed to load file into local database: {:?}\n Error: {}", file, error),
            }
        }
    }

    // If glacier_state is empty, populate it from Glacier.
    if glacier_state_is_empty(conn) {
        println!("Glacier state empty. Loading state from DynamoDB and S3...");
        let _ = restore::db_from_s3(args, conn, &s3_client, &dynamo_client).await;
    }

    // UPLOAD CHANGES
    let (successes, failures) = backup(args, conn, s3_client, dynamo_client).await;
    
    // CLEAR STATE 
    clear_local_state(conn);

    // PRINT RESULTS
    println!("Backup complete: {successes} succeeded, {failures} failed.");

    Ok(())
}


