mod blockchain;
mod cli;
mod config;
mod core;
mod crypto;
mod error;
mod logging;
mod network;
mod rpc;
mod storage;

use clap::Parser;
use cli::{Cli, Command};
use config::Config;
use error::AppError;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::StartNode);

    match command {
        Command::GenerateWallet { out, faucet } => cli::run_generate_wallet(&out, faucet),
        other => {
            let config = Config::load_from_path(&cli.config)?;
            logging::init_logging(&config.logging.level)?;
            cli::run(Some(other), config).await
        }
    }
}
