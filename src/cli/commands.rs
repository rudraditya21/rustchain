use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::DEFAULT_CONFIG_PATH;

#[derive(Debug, Parser)]
#[command(
    name = "rustchain",
    version,
    about = "A minimal production-oriented blockchain node"
)]
pub struct Cli {
    #[arg(long, default_value = DEFAULT_CONFIG_PATH, global = true)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    StartNode,
    Mine {
        #[arg(long)]
        rpc_url: Option<String>,
        #[arg(long)]
        timestamp_unix: Option<u64>,
        #[arg(long, default_value_t = 1_000_000)]
        max_nonce: u64,
    },
    Send {
        #[arg(long, default_value = "wallet.json")]
        wallet: PathBuf,
        #[arg(long)]
        rpc_url: Option<String>,
        #[arg(long)]
        to: String,
        #[arg(long)]
        amount: u64,
        #[arg(long, default_value_t = 0)]
        fee: u64,
        #[arg(long)]
        nonce: u64,
    },
    GenerateWallet {
        #[arg(long, default_value = "wallet.json")]
        out: PathBuf,
        #[arg(long, default_value_t = false)]
        faucet: bool,
    },
}
