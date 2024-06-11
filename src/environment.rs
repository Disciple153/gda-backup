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
const DB_USER: &str = "DB_USER";
const DB_PASSWORD: &str = "DB_PASSWORD";
const DB_HOST: &str = "DB_HOST";
const DB_NAME: &str = "DB_NAME";

#[derive(Debug, Parser, Clone)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    #[arg(long, default_value_t = false)]
    pub debug: bool,
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {

    /// Backups files using a config file and/or environment variables
    BackupWithEnv(BackupWithEnvArgs),

    /// Backups files
    Backup(BackupArgs),

    /// Restores files
    Restore(RestoreArgs),
    
    /// Cleans up dangling dynamo entries
    CleanDynamo(CleanDynamoArgs),

    /// Clears the local database
    ClearDatabase(ClearDatabaseArgs),
    
    /// Clears the remote data
    DeleteBackup(DeleteBackupArgs),
}

#[derive(Debug, Args, Clone)]
pub struct BackupWithEnvArgs {
    #[arg()]
    pub config_file: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct BackupArgs {
    #[arg(short = 't', long)]
    pub target_dir: String,

    #[arg(short = 'm', long, default_value_t = 180)]
    pub min_storage_duration: i64,
    #[arg(short = 'f', long)]
    pub filter: Vec<String>,

    #[arg(short = 'b', long)]
    bucket_name: String,
    #[arg(short = 'd', long)]
    dynamo_table: String,
    
    #[arg(short = 'e', long)]
    db_engine: String,
    #[arg(short = 'u', long)]
    db_user: String,
    #[arg(short = 'p', long)]
    db_password: String,
    #[arg(short = 'a', long)]
    db_host: String,
    #[arg(short = 'n', long)]
    db_name: String,
}

#[derive(Debug, Args, Clone)]
pub struct RestoreArgs {
    #[arg(short = 't', long)]
    pub target_dir: String,

    #[arg(short = 'b', long)]
    bucket_name: String,
    #[arg(short = 'd', long)]
    dynamo_table: String,
}

#[derive(Debug, Args, Clone)]
pub struct CleanDynamoArgs {
    #[arg(short = 'd', long)]
    pub dynamo_table: String,
}

#[derive(Debug, Args, Clone)]
pub struct ClearDatabaseArgs {
    #[arg(short = 'e', long)]
    db_engine: String,
    #[arg(short = 'u', long)]
    db_user: String,
    #[arg(short = 'p', long)]
    db_password: String,
    #[arg(short = 'a', long)]
    db_host: String,
    #[arg(short = 'n', long)]
    db_name: String,
}

#[derive(Debug, Args, Clone)]
pub struct DeleteBackupArgs {
    #[arg(short = 'b', long)]
    bucket_name: String,
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
    db_user: Option<String>,
    db_password: Option<String>,
    db_host: Option<String>,
    db_name: Option<String>,
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
            db_user: None,
            db_password: None,
            db_host: None,
            db_name: None,
            dry_run: None,
            log_level: None,
        }
    }
}

// GENERIC ARGUMENT STRUCTS

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
            Err(error) => {
                dbg!(error);
                return BackupWithEnvYaml::empty();
            },
        };

        yaml
    }
}

impl From<BackupWithEnvYaml> for BackupArgs {
    fn from(yaml: BackupWithEnvYaml) -> Self {

        dbg!(yaml.clone());

        let target_dir = get_var(yaml.target_dir, TARGET_DIR);
        let bucket_name = get_var(yaml.bucket_name, BUCKET_NAME);
        let dynamo_table = get_var(yaml.dynamo_table, DYNAMO_TABLE);
        let db_engine = get_var(yaml.db_engine, DB_ENGINE);
        let db_user = get_var(yaml.db_user, DB_USER);
        let db_password = get_var(yaml.db_password, DB_PASSWORD);
        let db_host = get_var(yaml.db_host, DB_HOST);
        let db_name = get_var(yaml.db_name, DB_NAME);

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
            db_user,
            db_password,
            db_host,
            db_name,
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

#[derive(Debug, Clone)]
pub struct DatabaseArgs {
    pub db_engine: String,
    pub db_user: String,
    pub db_password: String,
    pub db_host: String,
    pub db_name: String,
}

impl From<BackupArgs> for DatabaseArgs {
    fn from(value: BackupArgs) -> Self {
        DatabaseArgs {
            db_engine: value.db_engine,
            db_user: value.db_user,
            db_password: value.db_password,
            db_host: value.db_host,
            db_name: value.db_name,
        }
    }
}

impl From<ClearDatabaseArgs> for DatabaseArgs {
    fn from(value: ClearDatabaseArgs) -> Self {
        DatabaseArgs {
            db_engine: value.db_engine,
            db_user: value.db_user,
            db_password: value.db_password,
            db_host: value.db_host,
            db_name: value.db_name,
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