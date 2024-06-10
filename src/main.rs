use std::env;
use std::io::{self, Error};

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use clap::Parser;
use diesel::prelude::PgConnection;
use log::{LevelFilter, error, info};
use env_logger::Builder;

use gda_backup::environment::{
    Cli,
    Commands,
};

use gda_backup::{
    clear_glacier_state, clear_local_state, establish_connection, glacier_state_is_empty
};

use gda_backup::backup::{backup, load};

use gda_backup::restore;
use gda_backup::s3;
use gda_backup::dynamodb::{self, HashTracker};

#[tokio::main]
async fn main() -> Result<(), Error> {

    // ARGUMENTS
    let cli = Cli::parse();

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

    // EXECUTE COMMAND
    match cli.clone().command {
        Commands::Backup(ref mut args) => {
            // FIX ARGUMENTS
            args.target_dir = fix_target_dir(args.clone().target_dir)?;

            dbg!(args.clone().target_dir);
            
            // Connect to local database
            let conn: &mut PgConnection = &mut establish_connection(args.clone().into());
            
            // Clear local_state from database
            clear_local_state(conn);
            
            // Load files into database from disk
            load(args.clone(), conn);
            
            // If glacier_state is empty, populate it from Glacier.
            if glacier_state_is_empty(conn) {
                info!("Glacier state empty. Loading state from DynamoDB and S3...");
                let _ = restore::db_from_aws(cli.clone(), args.clone(), conn, &s3_client, &dynamo_client).await;
            }

            // UPLOAD CHANGES
            let (successes, failures) = backup(cli.clone(), args.clone(), conn, s3_client, dynamo_client).await;
            
            // CLEAR STATE 
            clear_local_state(conn);

            // PRINT RESULTS
            info!("Backup complete: {successes} succeeded, {failures} failed.");

        },
        Commands::Restore(mut args) => {
            // FIX ARGUMENTS
            args.target_dir = fix_target_dir(args.clone().target_dir)?;

            match restore::restore(cli, args, s3_client, dynamo_client).await {
                Ok((restored, failed)) => info!("Restore complete: {restored} restored, {failed} failed."),
                Err(error) => error!("Restore failed: {:?}", error),
            }
        },
        Commands::ClearDatabase(args) => {
            // Connect to local database
            let conn: &mut PgConnection = &mut establish_connection(args.clone().into());

            // Clear glacier state
            clear_glacier_state(conn);
        },
        Commands::DeleteBackup(ref mut args) => {
            // Get confirmation
            let mut buffer = String::new();
            let stdin = io::stdin();
            
            println!("Are you sure you want to delete your backup? (y/n)");
            stdin.read_line(&mut buffer)?;
            buffer.retain(|c| !c.is_whitespace());

            if buffer.to_lowercase() != "y" && buffer.to_lowercase() != "yes" {
                info!("Aborting...");
                return Ok(());
            }
            
            // Delete all items in DynamoDB
            match HashTracker::permanently_delete_all(args.clone().into(), dynamo_client).await {
                Ok(_) => info!("DynamoDB delete all succeeded."),
                Err(error) => error!("DynamoDB delete all failed: {:?}", error),
            };

            // Delete all items in S3
            match s3::permanently_delete_all(&s3_client, args.clone().into()).await {
                Ok(_) => info!("S3 delete all succeeded."),
                Err(error) => error!("S3 delete all failed: {:?}", error),
            };
        }
    }

    Ok(())
}


fn fix_target_dir(target_dir: String) -> Result<String, Error> {

    let target_dir = match target_dir.strip_suffix("/") {
        Some(s) => s.to_owned(),
        None => target_dir.clone()
    };

    Ok(match target_dir.strip_prefix("./") {
        Some(s) => {
            let path = env::current_dir()?.to_str().unwrap().to_string();
            path + "/" + s
        },
        None => target_dir.clone()
    })
}