use clap::Parser;
use rustchain::cli::{self, Cli, Command};
use rustchain::config::Config;
use rustchain::error::AppError;
use rustchain::logging;

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
