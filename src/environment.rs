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
    
    #[arg(short = 't', long)]
    pub target_dir: String,
    #[arg(short = 'b', long)]
    pub bucket_name: String,
    #[arg(short = 'd', long)]
    pub dynamo_table: String,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Backups files
    Backup(BackupArgs),

    /// Restores files
    Restore(RestoreArgs),
}

#[derive(Args, Clone)]
pub struct BackupArgs {

    #[arg(short = 'm', long, default_value_t = 180)]
    pub min_storage_duration: i64,
    
    #[arg(short = 'e', long)]
    pub db_engine: String,
    #[arg(short = 'u', long)]
    pub db_user: String,
    #[arg(short = 'p', long)]
    pub db_password: String,
    #[arg(short = 'a', long)]
    pub db_host: String,
    #[arg(short = 'n', long)]
    pub db_name: String,
}

#[derive(Args, Clone)]
pub struct RestoreArgs {
}