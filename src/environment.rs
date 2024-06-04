use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {

    #[arg(short = 't', long)]
    pub target_dir: String,
    #[arg(short = 'b', long)]
    pub bucket_name: String,
    
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

    #[arg(short = 'd', long, default_value_t = false)]
    pub dry_run: bool,
}