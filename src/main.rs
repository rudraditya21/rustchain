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
    let config = Config::load_from_path(&cli.config)?;

    logging::init_logging(&config.logging.level)?;

    match cli.command.unwrap_or(Command::StartNode) {
        Command::StartNode => run_node(config).await,
        Command::Mine => Err(AppError::NotImplemented("mine")),
        Command::Send {
            to: _,
            amount: _,
            fee: _,
            nonce: _,
        } => Err(AppError::NotImplemented("send")),
        Command::GenerateWallet => Err(AppError::NotImplemented("generate-wallet")),
    }
}

async fn run_node(config: Config) -> Result<(), AppError> {
    tracing::info!(
        node_id = %config.node.id,
        chain_id = %config.node.chain_id,
        p2p_addr = %config.network.listen_addr,
        rpc_addr = %config.rpc.listen_addr,
        "node started"
    );

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutdown signal received");

    Ok(())
}
