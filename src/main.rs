use std::env;
use std::io::{self, Error};

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use clap::Parser;
use diesel::prelude::PgConnection;
use log::{LevelFilter, error, info};
use env_logger::Builder;

use ntfy::{Auth, Dispatcher, Payload, Priority, dispatcher};

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
    let cli = Cli::parse();

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

/// The function `fix_target_dir` in Rust takes a target directory path as input,
/// removes any trailing slashes and leading "./", and returns the fixed path.
/// 
/// Arguments:
/// 
/// * `target_dir`: The function `fix_target_dir` takes a `target_dir` parameter of
/// type `String` and returns a `Result<String, Error>`. The function processes the
/// `target_dir` string by removing any trailing '/' characters and then checking if
/// it starts with './'. If it does, it appends
/// 
/// Returns:
/// 
/// The function `fix_target_dir` returns a `Result<String, Error>`, where the
/// `String` is the fixed target directory path and the `Error` type is not
/// specified in the code snippet provided.
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

/// The function `fix_filter` takes a `BackupArgs` struct as input and returns a
/// vector of strings based on certain conditions.
/// 
/// Arguments:
/// 
/// * `args`: ```rust
/// 
/// Returns:
/// 
/// The function `fix_filter` returns a `Vec<String>`. If the length of
/// `args.filter` is 1 and `args.filter_delimiter` is Some value, it splits the
/// first element of `args.filter` using the delimiter specified in
/// `args.filter_delimiter`, converts each split part to a String, and collects them
/// into a new vector of Strings. Otherwise, it returns the original
fn fix_filter(args: BackupArgs) -> Vec<String> {
    if args.filter.len() == 1 && args.filter_delimiter.is_some() {
        return args.filter[0].split(&args.filter_delimiter.unwrap()).map(|s| s.to_string()).collect();
    }
    else {
        return args.filter;
    }
}

/// The function `backup` in Rust handles backing up data to S3, providing 
/// notifications on success or failure.
/// 
/// Arguments:
/// 
/// * `cli`: The `cli` parameter in the `backup` function represents the
/// command-line interface (CLI) that is being used to interact with the backup
/// process. It likely contains information about the user input, options, and
/// commands provided by the user when running the backup operation. This parameter
/// allows the function to communicate
/// * `args`: The `args` parameter in the `backup` function seems to be a struct or
/// a set of arguments that are used throughout the backup process. It includes
/// fields like `target_dir`, `filter`, and possibly other configuration options
/// needed for the backup operation.
/// * `dispatcher`: The `dispatcher` parameter in the `backup` function is an
/// optional `Dispatcher` type. It seems to be used for dispatching notifications or
/// handling some kind of event dispatching within the backup process. If the
/// `dispatcher` is provided, it can be used to send notifications or trigger
/// certain actions
/// * `s3_client`: The `s3_client` parameter in the `backup` function is a mutable
/// reference to an instance of the `S3Client` struct. This client is used to
/// interact with Amazon S3 services for storing and retrieving data during the
/// backup process. The `&mut S3Client` type indicates
/// * `dynamo_client`: The `dynamo_client` parameter in the `backup` function is a
/// mutable reference to a `DynamoClient` instance. This client is used to interact
/// with DynamoDB, a fully managed NoSQL database service provided by AWS. The
/// `backup` function seems to be performing a backup operation that
/// 
/// Returns:
/// 
/// The `backup` function is returning a `Result<(), Error>`.
async fn backup(cli: Cli, mut args: BackupArgs, dispatcher: Option<Dispatcher<dispatcher::Async>>, s3_client: &mut S3Client, dynamo_client: &mut DynamoClient) -> Result<(), Error> {
    
    // FIX ARGUMENTS
    args.target_dir = fix_target_dir(args.target_dir.clone())?;
    args.filter = fix_filter(args.clone());

    ntfy(cli.clone(), dispatcher.clone(), "Backup starting", 
        format!("Starting backup of {}", args.target_dir.clone()), 
        Priority::Default
    ).await;

    // Connect to local database
    let conn: &mut PgConnection = &mut establish_connection(args.clone().into());
    
    // Clear local_state from database
    info!("Preparing to back up: Cleaning up previous backup data...");
    clear_local_state(conn);
    
    // Load files into database from disk
    info!("Preparing to back up: Loading all files...");
    backup::load(args.clone(), conn);
    
    // If glacier_state is empty, populate it from Glacier.
    if glacier_state_is_empty(conn) {
        info!("Glacier state empty. Loading state from DynamoDB and S3...");
        let _ = restore::postgres_from_aws(cli.clone(), args.clone(), conn, &s3_client, &dynamo_client).await;
    }

    // UPLOAD CHANGES
    let (successes, failures) = backup::backup(cli.clone(), args.clone(), conn, s3_client, dynamo_client).await;
    
    // CLEAR STATE 
    info!("Backup complete: Cleaning up...");
    clear_local_state(conn);

    // PRINT RESULTS
    info!("Backup complete: {successes} succeeded, {failures} failed.");

    if failures == 0 {
        ntfy(cli, dispatcher, "Backup complete", 
            format!("Completed backup of {}, {successes} succeeded, {failures} failed.", args.target_dir), 
            Priority::Default
        ).await;
    }
    else {
        ntfy(cli, dispatcher, "Backup complete with failures", 
            format!("Failed backup of {}, {successes} succeeded, {failures} failed.", args.target_dir),
            Priority::High
        ).await;
    }

    Ok(())
}

/// The function `restore` in Rust asynchronously restores data using S3 and
/// DynamoDB clients after fixing the target directory argument.
/// 
/// Arguments:
/// 
/// * `cli`: The `cli` parameter is of type `Cli`, which likely represents the
/// command-line interface for the application. It is used to interact with the
/// command-line arguments and options provided by the user when running the
/// program.
/// * `args`: The `args` parameter in the `restore` function is of type
/// `RestoreArgs` and represents the arguments needed for the restore operation. It
/// seems that the `target_dir` field of `args` is being updated by calling the
/// `fix_target_dir` function before proceeding with the restore operation.
/// * `s3_client`: The `s3_client` parameter is a mutable reference to an S3 client,
/// which is used to interact with Amazon S3 services for storing and retrieving
/// data. It allows the function to perform operations such as uploading,
/// downloading, and managing objects in S3 buckets.
/// * `dynamo_client`: The `dynamo_client` parameter in the `restore` function is a
/// mutable reference to a `DynamoClient` instance. This parameter allows the
/// function to interact with a DynamoDB client to perform operations such as
/// reading or writing data to a DynamoDB table.
/// 
/// Returns:
/// 
/// The `restore` function is returning a `Result<(), Error>`.
async fn restore(cli: Cli, mut args: RestoreArgs, s3_client: &mut S3Client, dynamo_client: &mut DynamoClient) -> Result<(), Error> {
    // FIX ARGUMENTS
    args.target_dir = fix_target_dir(args.clone().target_dir)?;

    match restore::restore(cli, args, s3_client, dynamo_client).await {
        Ok((restored, failed)) => info!("Restore complete: {restored} restored, {failed} failed."),
        Err(error) => error!("Restore failed: {:?}", error),
    };

    Ok(())
}

/// The `clean_dynamo` function in Rust asynchronously cleans up a DynamoDB table by
/// updating hash trackers associated with the table.
/// 
/// Arguments:
/// 
/// * `args`: The `args` parameter in the `clean_dynamo` function likely contains
/// information needed for cleaning up a DynamoDB table. It seems to include a
/// reference to the DynamoDB table that needs to be cleaned. The specific details
/// of the `CleanDynamoArgs` struct are not provided in the code
/// * `dynamo_client`: The `dynamo_client` parameter in the `clean_dynamo` function
/// is a mutable reference to a `DynamoClient` instance. This parameter allows the
/// function to interact with the DynamoDB service using the provided client. By
/// passing it as a mutable reference, the function can modify the client's
/// 
/// Returns:
/// 
/// The `clean_dynamo` function is returning a `Result<(), Error>`.
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

/// The `clear_database` function in Rust clears the glacier state in a local
/// database.
/// 
/// Arguments:
/// 
/// * `args`: The `args` parameter in the `clear_database` function likely
/// represents the arguments needed to configure and establish a connection to the
/// database. It seems to be of type `ClearDatabaseArgs`, which may contain
/// information such as database credentials, connection settings, or any other
/// necessary data for connecting to the database
/// 
/// Returns:
/// 
/// The `clear_database` function is returning a `Result<(), Error>`. This means
/// that it is returning a `Result` enum where the success case contains an empty
/// tuple `()` and the error case contains an `Error`.
fn clear_database(args: ClearDatabaseArgs) -> Result<(), Error> {
    // Connect to local database
    let conn: &mut PgConnection = &mut establish_connection(args.clone().into());

    // Clear glacier state
    clear_glacier_state(conn);

    Ok(())
}

/// The function `delete_backup` prompts the user for confirmation before deleting
/// all items in DynamoDB and S3.
/// 
/// Arguments:
/// 
/// * `args`: `DeleteBackupArgs` - Struct containing information needed for deleting
/// the backup, such as backup ID or other relevant data.
/// * `s3_client`: The `s3_client` parameter in the `delete_backup` function is a
/// mutable reference to an instance of the `S3Client` struct. This client is used
/// to interact with an S3 (Simple Storage Service) storage system, allowing the
/// function to perform operations such as deleting items from an
/// * `dynamo_client`: The `dynamo_client` parameter in the `delete_backup` function
/// is a mutable reference to a `DynamoClient` instance. This parameter allows the
/// function to interact with DynamoDB to delete items from the database. The
/// function uses this client to call the `permanently_delete_all` method
/// 
/// Returns:
/// 
/// The `delete_backup` function returns a `Result<(), Error>`.
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

/// The function `ntfy` sends a notification message with specified title, message,
/// and priority using a dispatcher if available.
/// 
/// Arguments:
/// 
/// * `cli`: The `cli` parameter seems to be of type `Cli`, which likely contains
/// information or configurations related to the command-line interface. It could
/// include settings, options, or other data relevant to the command-line
/// operations.
/// * `dispatcher`: The `dispatcher` parameter is an optional `Dispatcher` type that
/// is used to send notifications. If a `dispatcher` is provided, the function will
/// use it to send a notification message with the specified title, message, and
/// priority. If the `dispatcher` is `None`, no notification will be
/// * `title`: The `title` parameter is a reference to a string that represents the
/// title of the notification message. It is used to provide a brief description or
/// summary of the notification content.
/// * `message`: The `message` parameter in the `ntfy` function represents the
/// content of the notification message that will be sent. It is a `String` type,
/// which means it can hold a sequence of characters to be included in the
/// notification.
/// * `priority`: The `priority` parameter in the function `ntfy` represents the
/// priority level of the notification being sent. It is used to indicate the
/// importance or urgency of the notification. The priority can be set to different
/// levels such as low, medium, high, or critical, depending on the system's
/// requirements
async fn ntfy(cli: Cli, dispatcher: Option<Dispatcher<dispatcher::Async>>, title: &str, message: String, priority: Priority) {
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

/// The function `ntfy_dispatcher` checks if necessary parameters are provided to
/// create a notification dispatcher and returns an optional dispatcher object.
/// 
/// Arguments:
/// 
/// * `cli`: The `cli` parameter in the `ntfy_dispatcher` function seems to be a
/// struct or object that contains the following fields:
/// 
/// Returns:
/// 
/// The function `ntfy_dispatcher` returns an `Option<Dispatcher>`. If the `cli`
/// argument does not have a `ntfy_topic` or `ntfy_url`, it returns `None`.
/// Otherwise, it creates a `Dispatcher` using the provided `ntfy_url` and
/// optionally sets credentials if `ntfy_username` and `ntfy_password` are provided.
/// Finally, it attempts
fn ntfy_dispatcher(cli: Cli) -> Option<Dispatcher<dispatcher::Async>> {

    if !cli.ntfy_topic.is_some() || !cli.ntfy_url.is_some() {
        info!("ntfy disabled. Missing url or topic.");
        return None;
    }

    let mut dispatcher = dispatcher::builder(cli.ntfy_url.unwrap());

    if cli.ntfy_username.is_some() && cli.ntfy_password.is_some() {
        dispatcher = dispatcher.credentials(Auth::credentials(cli.ntfy_username.unwrap(), cli.ntfy_password.unwrap()))
    };

    dispatcher.build_async().ok()
}