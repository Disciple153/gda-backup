use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser, Clone)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Set dry run to true to view the list of files that would be backed up without uploading anything. 
    #[arg(long, default_value_t = false, env)]
    pub dry_run: bool,

    /// Enable debug for more verbose logs.
    #[arg(long, default_value_t = false, env)]
    pub debug: bool,
    /// Enable quiet to only display errors.
    #[arg(long, default_value_t = false, env)]
    pub quiet: bool,

    /// ntfy url.
    #[arg(long, env)]
    pub ntfy_url: Option<String>,

    /// ntfy username.
    #[arg(long, env)]
    pub ntfy_username: Option<String>,

    /// ntfy password.
    #[arg(long, env)]
    pub ntfy_password: Option<String>,

    /// ntfy topic.
    #[arg(long, env)]
    pub ntfy_topic: Option<String>,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {

    /// Backups files.
    Backup(BackupArgs),

    /// Restores files.
    Restore(RestoreArgs),
    
    /// Cleans up dangling dynamo entries.
    CleanDynamo(CleanDynamoArgs),

    /// Clears the local database.
    ClearDatabase(ClearDatabaseArgs),
    
    /// Clears the remote data.
    DeleteBackup(DeleteBackupArgs),
}

#[derive(Debug, Args, Clone)]
pub struct BackupArgs {
    /// The directory targeted by the backup.  
    #[arg(short = 't', long, env)]
    pub target_dir: String,

    /// The length of time after an object is created before it will be deleted by S3 lifecycle configurations.
    #[arg(short = 'm', long, env)]
    pub min_storage_duration: Option<i64>,
    /// A list of regular expressions used to filter files out of backups.
    #[arg(short = 'f', long, env)]
    pub filter: Vec<String>,
    /// A delimiter that if supplied, can be used to split "FILTER" into multiple regex strings.
    #[arg(short = 's', long, env)]
    pub filter_delimiter: Option<String>,

    /// The S3 bucket to which backups will be uploaded. 
    #[arg(short = 'b', long, env)]
    bucket_name: String,
    /// The DynamoDB table which will store backup related metadata.
    #[arg(short = 'd', long, env)]
    dynamo_table: String,
    
    /// The engine of the local database. (Only postgres is supported.)
    #[arg(short = 'e', long, env)]
    db_engine: String,
    /// The username of the postgres database.
    #[arg(short = 'u', long, env)]
    postgres_user: String,
    /// The password to the postgres database.
    #[arg(short = 'p', long, env)]
    postgres_password: String,
    /// The hostname of the postgres database.
    #[arg(short = 'a', long, env)]
    postgres_host: String,
    /// The name of the postgres database.
    #[arg(short = 'n', long, env)]
    postgres_db: String,
}

#[derive(Debug, Args, Clone)]
pub struct RestoreArgs {
    /// The directory targeted by the backup.  
    #[arg(short = 't', long, env)]
    pub target_dir: String,

    /// The S3 bucket which contains your backup. 
    #[arg(short = 'b', long, env)]
    bucket_name: String,
    /// The DynamoDB contains your backup metadata.
    #[arg(short = 'd', long, env)]
    dynamo_table: String,
}

#[derive(Debug, Args, Clone)]
pub struct CleanDynamoArgs {
    /// The DynamoDB contains your backup metadata.
    #[arg(short = 'd', long, env)]
    pub dynamo_table: String,
}

#[derive(Debug, Args, Clone)]
pub struct ClearDatabaseArgs {
    /// The engine of the local database. (Only postgres is supported.)
    #[arg(short = 'e', long, env)]
    db_engine: String,
    /// The username of the postgres database.
    #[arg(short = 'u', long, env)]
    postgres_user: String,
    /// The password to the postgres database.
    #[arg(short = 'p', long, env)]
    postgres_password: String,
    /// The hostname of the postgres database.
    #[arg(short = 'a', long, env)]
    postgres_host: String,
    /// The name of the postgres database.
    #[arg(short = 'n', long, env)]
    postgres_db: String,
}

#[derive(Debug, Args, Clone)]
pub struct DeleteBackupArgs {
    /// The S3 bucket which contains your backup. 
    #[arg(short = 'b', long, env)]
    bucket_name: String,
    /// The DynamoDB contains your backup metadata.
    #[arg(short = 'd', long, env)]
    dynamo_table: String,
}

// GENERIC ARGUMENT STRUCTS

#[derive(Debug, Clone)]
pub struct DatabaseArgs {
    pub db_engine: String,
    pub postgres_user: String,
    pub postgres_password: String,
    pub postgres_host: String,
    pub postgres_db: String,
}

impl From<BackupArgs> for DatabaseArgs {
    fn from(value: BackupArgs) -> Self {
        DatabaseArgs {
            db_engine: value.db_engine,
            postgres_user: value.postgres_user,
            postgres_password: value.postgres_password,
            postgres_host: value.postgres_host,
            postgres_db: value.postgres_db,
        }
    }
}

impl From<ClearDatabaseArgs> for DatabaseArgs {
    fn from(value: ClearDatabaseArgs) -> Self {
        DatabaseArgs {
            db_engine: value.db_engine,
            postgres_user: value.postgres_user,
            postgres_password: value.postgres_password,
            postgres_host: value.postgres_host,
            postgres_db: value.postgres_db,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AwsArgs {
    pub bucket_name: String,
    pub dynamo_table: String,
}

impl From<BackupArgs> for AwsArgs {
    fn from(value: BackupArgs) -> Self {
        AwsArgs {
            bucket_name: value.bucket_name,
            dynamo_table: value.dynamo_table,
        }
    }
}

impl From<RestoreArgs> for AwsArgs {
    fn from(value: RestoreArgs) -> Self {
        AwsArgs {
            bucket_name: value.bucket_name,
            dynamo_table: value.dynamo_table,
        }
    }
}

impl From<DeleteBackupArgs> for AwsArgs {
    fn from(value: DeleteBackupArgs) -> Self {
        AwsArgs {
            bucket_name: value.bucket_name,
            dynamo_table: value.dynamo_table,
        }
    }
}