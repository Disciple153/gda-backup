use std::env;
use std::io::{self, Error};

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use clap::Parser;
use diesel::prelude::PgConnection;
use log::{LevelFilter, error, info};
use env_logger::Builder;

use ntfy::{Auth, Dispatcher, Payload, Priority};

use gda_backup::environment::{
    AwsArgs, BackupArgs, CleanDynamoArgs, ClearDatabaseArgs, Cli, Commands, DeleteBackupArgs, RestoreArgs
};

use gda_backup::{
    clear_glacier_state, clear_local_state, establish_connection, glacier_state_is_empty
};

use gda_backup::backup;

use gda_backup::restore;
use gda_backup::s3;
use gda_backup::dynamodb::{self, HashTracker};

#[tokio::main]
async fn main() -> Result<(), Error> {

    // ARGUMENTS
    let mut cli = Cli::parse();

    if let Commands::BackupWithEnv(_) = cli.command {
        cli.get_env();
    }

    let cli = cli;


    let dispatcher = ntfy_dispatcher(cli.clone());

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
        Commands::BackupWithEnv(args) => {
            backup(cli, args.into(), dispatcher, s3_client, dynamo_client).await?;
        },
        Commands::Backup(args) => {
            backup(cli, args, dispatcher, s3_client, dynamo_client).await?;
        },
        Commands::Restore(args) => {
            restore(cli, args, s3_client, dynamo_client).await?;
        },
        Commands::CleanDynamo(args) => {
            clean_dynamo(args, dynamo_client).await?;
        }
        Commands::ClearDatabase(args) => {
            clear_database(args)?;
        },
        Commands::DeleteBackup(args) => {
            delete_backup(args, s3_client, dynamo_client).await?;
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

async fn backup(cli: Cli, mut args: BackupArgs, dispatcher: Option<Dispatcher>, s3_client: &mut S3Client, dynamo_client: &mut DynamoClient) -> Result<(), Error> {
    // FIX ARGUMENTS
    args.target_dir = fix_target_dir(args.target_dir.clone())?;

    // Connect to local database
    let conn: &mut PgConnection = &mut establish_connection(args.clone().into());
    
    // Clear local_state from database
    clear_local_state(conn);
    
    // Load files into database from disk
    backup::load(args.clone(), conn);
    
    // If glacier_state is empty, populate it from Glacier.
    if glacier_state_is_empty(conn) {
        info!("Glacier state empty. Loading state from DynamoDB and S3...");
        let _ = restore::postgres_from_aws(cli.clone(), args.clone(), conn, &s3_client, &dynamo_client).await;
    }

    // UPLOAD CHANGES
    let (successes, failures) = backup::backup(cli.clone(), args.clone(), conn, s3_client, dynamo_client).await;
    
    // CLEAR STATE 
    clear_local_state(conn);

    // PRINT RESULTS
    info!("Backup complete: {successes} succeeded, {failures} failed.");

    if failures == 0 {
        ntfy(cli, dispatcher, "Backup complete", 
            format!("{successes} succeeded, {failures} failed."), 
            Priority::Default
        ).await;
    }
    else {
        ntfy(cli, dispatcher, "Backup complete with failures", 
            format!("{successes} succeeded, {failures} failed."),
            Priority::Default
        ).await;
    }

    Ok(())
}

async fn restore(cli: Cli, mut args: RestoreArgs, s3_client: &mut S3Client, dynamo_client: &mut DynamoClient) -> Result<(), Error> {
    // FIX ARGUMENTS
    args.target_dir = fix_target_dir(args.clone().target_dir)?;

    match restore::restore(cli, args, s3_client, dynamo_client).await {
        Ok((restored, failed)) => info!("Restore complete: {restored} restored, {failed} failed."),
        Err(error) => error!("Restore failed: {:?}", error),
    };

    Ok(())
}

async fn clean_dynamo(args: CleanDynamoArgs, dynamo_client: &mut DynamoClient) -> Result<(), Error> {
    let aws_args = AwsArgs {
        bucket_name: "".to_string(),
        dynamo_table: args.dynamo_table.clone()
    };
    
    let hash_trackers = HashTracker::get_all(&dynamo_client, aws_args.clone())
        .await.unwrap_or(vec![]);

    for hash_tracker in hash_trackers {
        let _ = hash_tracker.update(aws_args.clone(), &dynamo_client).await;
    }

    Ok(())
}

fn clear_database(args: ClearDatabaseArgs) -> Result<(), Error> {
    // Connect to local database
    let conn: &mut PgConnection = &mut establish_connection(args.clone().into());

    // Clear glacier state
    clear_glacier_state(conn);

    Ok(())
}

async fn delete_backup(args: DeleteBackupArgs, s3_client: &mut S3Client, dynamo_client: &mut DynamoClient) -> Result<(), Error> {
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

    Ok(())
}

async fn ntfy(cli: Cli, dispatcher: Option<Dispatcher>, title: &str, message: String, priority: Priority) {
    if let Some(dispatcher) = dispatcher {
        let result = dispatcher.send(&Payload::new(cli.ntfy_topic.unwrap())
            .message(message) // Add optional message
            .title(title) // Add optional title
            .priority(priority) // Edit priority
            .markdown(true)
        ).await; // Use markdown).await.unwrap();

        if let Err(error) = result {
            error!("Failed to send ntfy message: {:?}", error);
        };
    };
}

fn ntfy_dispatcher(cli: Cli) -> Option<Dispatcher> {

    if !cli.ntfy_topic.is_some() || !cli.ntfy_url.is_some() {
        info!("ntfy disabled. Missing url or topic.");
        return None;
    }

    let mut dispatcher = Dispatcher::builder(cli.ntfy_url.unwrap());

    if cli.ntfy_username.is_some() && cli.ntfy_password.is_some() {
        dispatcher = dispatcher.credentials(Auth::new(cli.ntfy_username.unwrap(), cli.ntfy_password.unwrap()))
    };

    dispatcher.build().ok()
}