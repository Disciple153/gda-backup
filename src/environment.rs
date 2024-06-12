use std::env;

use clap::{Args, Parser, Subcommand};
use serde::Deserialize;

const DRY_RUN: &str = "DRY_RUN";
const DEBUG: &str = "debug";
const QUIET: &str = "quiet";
const LOG_LEVEL: &str = "LOG_LEVEL";
const TARGET_DIR: &str = "TARGET_DIR";
const MIN_STORAGE_DURATION: &str = "MIN_STORAGE_DURATION";
const FILTER: &str = "FILTER";
const BUCKET_NAME: &str = "BUCKET_NAME";
const DYNAMO_TABLE: &str = "DYNAMO_TABLE";
const DB_ENGINE: &str = "DB_ENGINE";
const POSTGRES_USER: &str = "POSTGRES_USER";
const POSTGRES_PASSWORD: &str = "POSTGRES_PASSWORD";
const POSTGRES_HOST: &str = "POSTGRES_HOST";
const POSTGRES_DB: &str = "POSTGRES_DB";

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
    /// The path to a yaml file containing backup arguments. This file contains all of the same arguments as the backup command.
    #[arg()]
    pub config_file: Option<String>,
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


// YAML SPEC
#[derive(Debug, Deserialize, Clone)]
pub struct BackupWithEnvYaml {
    dry_run: Option<bool>,
    log_level: Option<String>,
    target_dir: Option<String>,
    min_storage_duration: Option<i64>,
    filter: Option<Vec<String>>,
    bucket_name: Option<String>,
    dynamo_table: Option<String>,
    db_engine: Option<String>,
    postgres_user: Option<String>,
    postgres_password: Option<String>,
    postgres_host: Option<String>,
    postgres_db: Option<String>,
}

// STRUCT CONSTRUCTORS
impl BackupWithEnvYaml {
    pub fn empty() -> BackupWithEnvYaml {
        BackupWithEnvYaml {
            target_dir: None,
            min_storage_duration: None,
            filter: None,
            bucket_name: None,
            dynamo_table: None,
            db_engine: None,
            postgres_user: None,
            postgres_password: None,
            postgres_host: None,
            postgres_db: None,
            dry_run: None,
            log_level: None,
        }
    }
}

// YAML STRUCTS AND HELPERS

impl From<BackupWithEnvArgs> for BackupWithEnvYaml {
    fn from(args: BackupWithEnvArgs) -> Self {

        let Some(config_path) = args.config_file else {
            return BackupWithEnvYaml::empty();
        };

        let Ok(config_file) = std::fs::File::open(config_path) else {
            return BackupWithEnvYaml::empty();
        };

        let yaml = match serde_yml::from_reader(config_file) {
            Ok(value) => value,
            Err(_) => return BackupWithEnvYaml::empty()
        };

        yaml
    }
}

impl From<BackupWithEnvYaml> for BackupArgs {
    fn from(yaml: BackupWithEnvYaml) -> Self {

        let target_dir = get_var(yaml.target_dir, TARGET_DIR);
        let bucket_name = get_var(yaml.bucket_name, BUCKET_NAME);
        let dynamo_table = get_var(yaml.dynamo_table, DYNAMO_TABLE);
        let db_engine = get_var(yaml.db_engine, DB_ENGINE);
        let postgres_user = get_var(yaml.postgres_user, POSTGRES_USER);
        let postgres_password = get_var(yaml.postgres_password, POSTGRES_PASSWORD);
        let postgres_host = get_var(yaml.postgres_host, POSTGRES_HOST);
        let postgres_db = get_var(yaml.postgres_db, POSTGRES_DB);

        let min_storage_duration = match yaml.min_storage_duration {
            Some(value) => value,
            None => match env::var(MIN_STORAGE_DURATION) {
                Ok(value) => match value.parse::<i64>() {
                    Ok(value) => value,
                    Err(_) => panic!("MIN_STORAGE_DURATION must be an integer"),
                },
                Err(_) => panic!("Missing environment variable: {MIN_STORAGE_DURATION}"),
            }
        };

        let filter = match yaml.filter {
            Some(value) => value,
            None => match env::var(FILTER) {
                Ok(value) => vec![value],
                Err(_) => vec![],
            },
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

impl From<BackupWithEnvYaml> for Cli {
    fn from(yaml: BackupWithEnvYaml) -> Self {

        let dry_run = get_var_bool(yaml.dry_run.clone(), DRY_RUN);
        let log_level = get_var(yaml.log_level.clone(), LOG_LEVEL);

        Cli {
            command: Commands::Backup(yaml.into()),
            dry_run,
            debug: log_level.to_lowercase() == DEBUG,
            quiet: log_level.to_lowercase() == QUIET,
        }
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

fn get_var_bool(yaml_value: Option<bool>, env_key: &str) -> bool {
    match yaml_value {
        Some(value) => value,
        None => match env::var(env_key) {
            Ok(value) => value.to_lowercase() == "true",
            Err(_) => false,
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