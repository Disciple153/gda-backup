use std::env;

use clap::{Args, Parser, Subcommand};

const DRY_RUN: &str = "DRY_RUN";
const DEBUG: &str = "debug";
const QUIET: &str = "quiet";
const LOG_LEVEL: &str = "LOG_LEVEL";
const TARGET_DIR: &str = "TARGET_DIR";
const MIN_STORAGE_DURATION: &str = "MIN_STORAGE_DURATION";
const FILTER: &str = "FILTER";
const FILTER_DELIMITER: &str = "FILTER_DELIMITER";
const BUCKET_NAME: &str = "BUCKET_NAME";
const DYNAMO_TABLE: &str = "DYNAMO_TABLE";
const DB_ENGINE: &str = "DB_ENGINE";
const POSTGRES_USER: &str = "POSTGRES_USER";
const POSTGRES_PASSWORD: &str = "POSTGRES_PASSWORD";
const POSTGRES_HOST: &str = "POSTGRES_HOST";
const POSTGRES_DB: &str = "POSTGRES_DB";

const NTFY_URL: &str = "NTFY_URL";
const NTFY_USERNAME: &str = "NTFY_USERNAME";
const NTFY_PASSWORD: &str = "NTFY_PASSWORD";
const NTFY_TOPIC: &str = "NTFY_TOPIC";

#[derive(Debug, Parser, Clone)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Set dry run to true to view the list of files that would be backed up without uploading anything. 
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Enable debug for more verbose logs.
    #[arg(long, default_value_t = false)]
    pub debug: bool,
    /// Enable quiet to only display errors.
    #[arg(long, default_value_t = false)]
    pub quiet: bool,

    /// ntfy url.
    #[arg(long)]
    pub ntfy_url: Option<String>,

    /// ntfy username.
    #[arg(long)]
    pub ntfy_username: Option<String>,

    /// ntfy password.
    #[arg(long)]
    pub ntfy_password: Option<String>,

    /// ntfy topic.
    #[arg(long)]
    pub ntfy_topic: Option<String>,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {

    /// Backups files using a config file and/or environment variables.
    BackupWithEnv(BackupWithEnvArgs),

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
pub struct BackupWithEnvArgs {
        /// The directory targeted by the backup.  
        #[arg(short = 't', long)]
        pub target_dir: Option<String>,
    
        /// The length of time after an object is created before it will be deleted by S3 lifecycle configurations.
        #[arg(short = 'm', long)]
        pub min_storage_duration: Option<i64>,
        /// A list of regular expressions used to filter files out of backups.
        #[arg(short = 'f', long)]
        pub filter: Vec<String>,
    
        /// The S3 bucket to which backups will be uploaded. 
        #[arg(short = 'b', long)]
        bucket_name: Option<String>,
        /// The DynamoDB table which will store backup related metadata.
        #[arg(short = 'd', long)]
        dynamo_table: Option<String>,
        
        /// The engine of the local database. (Only postgres is supported.)
        #[arg(short = 'e', long)]
        db_engine: Option<String>,
        /// The username of the postgres database.
        #[arg(short = 'u', long)]
        postgres_user: Option<String>,
        /// The password to the postgres database.
        #[arg(short = 'p', long)]
        postgres_password: Option<String>,
        /// The hostname of the postgres database.
        #[arg(short = 'a', long)]
        postgres_host: Option<String>,
        /// The name of the postgres database.
        #[arg(short = 'n', long)]
        postgres_db: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct BackupArgs {
    /// The directory targeted by the backup.  
    #[arg(short = 't', long)]
    pub target_dir: String,

    /// The length of time after an object is created before it will be deleted by S3 lifecycle configurations.
    #[arg(short = 'm', long, default_value_t = 180)]
    pub min_storage_duration: i64,
    /// A list of regular expressions used to filter files out of backups.
    #[arg(short = 'f', long)]
    pub filter: Vec<String>,

    /// The S3 bucket to which backups will be uploaded. 
    #[arg(short = 'b', long)]
    bucket_name: String,
    /// The DynamoDB table which will store backup related metadata.
    #[arg(short = 'd', long)]
    dynamo_table: String,
    
    /// The engine of the local database. (Only postgres is supported.)
    #[arg(short = 'e', long)]
    db_engine: String,
    /// The username of the postgres database.
    #[arg(short = 'u', long)]
    postgres_user: String,
    /// The password to the postgres database.
    #[arg(short = 'p', long)]
    postgres_password: String,
    /// The hostname of the postgres database.
    #[arg(short = 'a', long)]
    postgres_host: String,
    /// The name of the postgres database.
    #[arg(short = 'n', long)]
    postgres_db: String,
}

#[derive(Debug, Args, Clone)]
pub struct RestoreArgs {
    /// The directory targeted by the backup.  
    #[arg(short = 't', long)]
    pub target_dir: String,

    /// The S3 bucket which contains your backup. 
    #[arg(short = 'b', long)]
    bucket_name: String,
    /// The DynamoDB contains your backup metadata.
    #[arg(short = 'd', long)]
    dynamo_table: String,
}

#[derive(Debug, Args, Clone)]
pub struct CleanDynamoArgs {
    /// The DynamoDB contains your backup metadata.
    #[arg(short = 'd', long)]
    pub dynamo_table: String,
}

#[derive(Debug, Args, Clone)]
pub struct ClearDatabaseArgs {
    /// The engine of the local database. (Only postgres is supported.)
    #[arg(short = 'e', long)]
    db_engine: String,
    /// The username of the postgres database.
    #[arg(short = 'u', long)]
    postgres_user: String,
    /// The password to the postgres database.
    #[arg(short = 'p', long)]
    postgres_password: String,
    /// The hostname of the postgres database.
    #[arg(short = 'a', long)]
    postgres_host: String,
    /// The name of the postgres database.
    #[arg(short = 'n', long)]
    postgres_db: String,
}

#[derive(Debug, Args, Clone)]
pub struct DeleteBackupArgs {
    /// The S3 bucket which contains your backup. 
    #[arg(short = 'b', long)]
    bucket_name: String,
    /// The DynamoDB contains your backup metadata.
    #[arg(short = 'd', long)]
    dynamo_table: String,
}


impl From<BackupWithEnvArgs> for BackupArgs {
    fn from(env_args: BackupWithEnvArgs) -> Self {

        let target_dir = get_var(env_args.target_dir, TARGET_DIR);
        let bucket_name = get_var(env_args.bucket_name, BUCKET_NAME);
        let dynamo_table = get_var(env_args.dynamo_table, DYNAMO_TABLE);
        let db_engine = get_var(env_args.db_engine, DB_ENGINE);
        let postgres_user = get_var(env_args.postgres_user, POSTGRES_USER);
        let postgres_password = get_var(env_args.postgres_password, POSTGRES_PASSWORD);
        let postgres_host = get_var(env_args.postgres_host, POSTGRES_HOST);
        let postgres_db = get_var(env_args.postgres_db, POSTGRES_DB);
        let filter_delimiter = env::var(FILTER_DELIMITER).ok();

        let min_storage_duration = match env_args.min_storage_duration {
            Some(value) => value,
            None => match env::var(MIN_STORAGE_DURATION) {
                Ok(value) => match value.parse::<i64>() {
                    Ok(value) => value,
                    Err(_) => panic!("MIN_STORAGE_DURATION must be an integer"),
                },
                Err(_) => panic!("Missing environment variable: {MIN_STORAGE_DURATION}"),
            }
        };

        let filter = if env_args.filter.len() > 0 {
            env_args.filter
        }
        else {
            match env::var(FILTER) {
                Ok(value) => {
                    match filter_delimiter {
                        Some(delimiter) => value.split(&delimiter).map(|v| v.to_string()).collect(),
                        None => vec![value],
                    }
                },
                Err(_) => vec![],
            }
        };

        BackupArgs {
            target_dir,
            min_storage_duration,
            filter,
            bucket_name,
            dynamo_table,
            db_engine,
            postgres_user,
            postgres_password,
            postgres_host,
            postgres_db,
        }
    }
}

impl Cli {
    pub fn get_env(&mut self) {
        self.dry_run = match env::var(DRY_RUN) {
            Ok(value) => {
                if value.to_lowercase() == "true" {
                    true
                }
                else if value.to_lowercase() == "false" {
                    false
                }
                else {
                    self.dry_run
                }
            },
            Err(_) => self.dry_run,
        };
        
        if let Ok(value) = env::var(LOG_LEVEL) {
            if value.to_lowercase() == DEBUG {
                self.debug = true;
                self.quiet = false;
            }
            else if value.to_lowercase() == QUIET {
                self.debug = false;
                self.quiet = true;
            }
        };

        self.ntfy_url = match &self.ntfy_url {
            Some(value) => Some(value.to_string()),
            None => env::var(NTFY_URL).ok(),
        };

        self.ntfy_topic = match &self.ntfy_topic {
            Some(value) => Some(value.to_string()),
            None => env::var(NTFY_TOPIC).ok(),
        };

        self.ntfy_username = match &self.ntfy_username {
            Some(value) => Some(value.to_string()),
            None => env::var(NTFY_USERNAME).ok(),
        };

        self.ntfy_password = match &self.ntfy_password {
            Some(value) => Some(value.to_string()),
            None => env::var(NTFY_PASSWORD).ok(),
        };
    }
}

fn get_var(yaml_value: Option<String>, env_key: &str) -> String {
    match yaml_value {
        Some(value) => value,
        None => match env::var(env_key) {
            Ok(value) => value,
            Err(_) => panic!("Missing environment variable: {env_key}"),
        }
    }
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