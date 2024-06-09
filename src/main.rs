use std::io::Error;

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use clap::Parser;
use diesel::prelude::PgConnection;
use log::{LevelFilter, error};
use env_logger::Builder;

use glacier_sync::environment::{
    Cli,
    Commands,
};

use glacier_sync::{
    clear_local_state,
    establish_connection,
    glacier_state_is_empty,
};

use glacier_sync::backup::{backup, load};

use glacier_sync::restore;
use glacier_sync::s3;
use glacier_sync::dynamodb;

#[tokio::main]
async fn main() -> Result<(), Error> {

    // ARGUMENTS
    let mut cli = Cli::parse();

    // SET LOG LEVEL

    if cli.quiet {
        Builder::new().filter_level(LevelFilter::Error).init();
    }
    else if cli.debug {
        Builder::new().filter_level(LevelFilter::Debug).init();
    }
    else {
        Builder::new().filter_level(LevelFilter::Info).init();
    }
    
    // GET CONNECTIONS
    let s3_client: &mut S3Client = &mut s3::get_client().await;
    let dynamo_client: &mut DynamoClient = &mut dynamodb::get_client().await;

    // FIX ARGUMENTS
    cli.target_dir = match cli.target_dir.strip_suffix("/") {
        Some(s) => s.to_owned(),
        None => cli.target_dir
    };

    // EXECUTE COMMAND
    match cli.command {
        Commands::Backup(ref args) => {
            
            // Connect to local database
            let conn: &mut PgConnection = &mut establish_connection(args.clone());
            
            // Clear local_state from database
            clear_local_state(conn);
            
            // Load files into database from disk
            load(cli.clone(), conn);
            
            // If glacier_state is empty, populate it from Glacier.
            if glacier_state_is_empty(conn) {
                println!("Glacier state empty. Loading state from DynamoDB and S3...");
                let _ = restore::db_from_s3(cli.clone(), conn, &s3_client, &dynamo_client).await;
            }

            // UPLOAD CHANGES
            let (successes, failures) = backup(cli.clone(), args.clone(), conn, s3_client, dynamo_client).await;
            
            // CLEAR STATE 
            clear_local_state(conn);

            // PRINT RESULTS
            println!("Backup complete: {successes} succeeded, {failures} failed.");

        },
        Commands::Restore(_) => {
            match restore::restore(cli, s3_client, dynamo_client).await {
                Ok((restored, failed)) => println!("Restore complete: {restored} restored, {failed} failed."),
                Err(error) => error!("Restore failed: {}", error),
            }
        },
    }

    Ok(())
}


