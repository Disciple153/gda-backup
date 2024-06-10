use clap::{Args, Parser, Subcommand};

#[derive(Parser, Clone)]
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

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Backups files
    Backup(BackupArgs),

    /// Restores files
    Restore(RestoreArgs),

    /// Clears the local database
    ClearDatabase(ClearDatabaseArgs),
    
    /// Clears the remote data
    DeleteBackup(DeleteBackupArgs),
}

#[derive(Args, Clone)]
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

#[derive(Args, Clone)]
pub struct RestoreArgs {
    #[arg(short = 't', long)]
    pub target_dir: String,

    #[arg(short = 'b', long)]
    bucket_name: String,
    #[arg(short = 'd', long)]
    dynamo_table: String,
}

#[derive(Args, Clone)]
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

#[derive(Args, Clone)]
pub struct DeleteBackupArgs {
    #[arg(short = 'b', long)]
    bucket_name: String,
    #[arg(short = 'd', long)]
    dynamo_table: String,
}

// GENERIC ARGUMENT STRUCTS

#[derive(Clone)]
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

#[derive(Clone)]
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