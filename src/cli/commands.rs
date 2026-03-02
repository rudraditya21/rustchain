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
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    StartNode,
    Mine,
    Send {
        #[arg(long)]
        to: String,
        #[arg(long)]
        amount: u64,
        #[arg(long, default_value_t = 0)]
        fee: u64,
        #[arg(long)]
        nonce: u64,
    },
    GenerateWallet,
}
