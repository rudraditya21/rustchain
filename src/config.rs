use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::AppError;

pub const DEFAULT_CONFIG_PATH: &str = "config/default.toml";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub node: NodeConfig,
    pub network: NetworkConfig,
    pub rpc: RpcConfig,
    pub storage: StorageConfig,
    pub mining: MiningConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct NodeConfig {
    pub id: String,
    pub chain_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    pub listen_addr: String,
    pub peers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RpcConfig {
    pub listen_addr: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StorageConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MiningConfig {
    pub difficulty_bits: u32,
    pub max_transactions_per_block: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    pub level: String,
}

impl Config {
    #[allow(dead_code)]
    pub fn load_default() -> Result<Self, AppError> {
        Self::load_from_path(DEFAULT_CONFIG_PATH)
    }

    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let path_ref = path.as_ref();

        if !path_ref.exists() {
            return Err(AppError::ConfigNotFound(path_ref.to_path_buf()));
        }

        let raw = fs::read_to_string(path_ref)?;
        let config: Self = toml::from_str(&raw)?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::Config;
    use crate::error::AppError;

    fn valid_toml() -> &'static str {
        r#"
[node]
id = "node-a"
chain_id = "testnet"

[network]
listen_addr = "127.0.0.1:6000"
peers = ["127.0.0.1:6001"]

[rpc]
listen_addr = "127.0.0.1:7000"

[storage]
path = "./tmp"

[mining]
difficulty_bits = 18
max_transactions_per_block = 500

[logging]
level = "debug"
"#
    }

    #[test]
    fn parse_config_from_file() -> Result<(), AppError> {
        let dir = tempdir()?;
        let path = dir.path().join("node.toml");
        fs::write(&path, valid_toml())?;

        let config = Config::load_from_path(&path)?;

        assert_eq!(config.node.id, "node-a");
        assert_eq!(config.network.peers.len(), 1);
        assert_eq!(config.mining.max_transactions_per_block, 500);
        assert_eq!(config.logging.level, "debug");

        Ok(())
    }

    #[test]
    fn missing_config_path_returns_error() {
        let error = Config::load_from_path("/definitely/missing/config.toml");

        match error {
            Ok(_) => panic!("expected missing config path to fail"),
            Err(AppError::ConfigNotFound(_)) => {}
            Err(other) => panic!("unexpected error variant: {other}"),
        }
    }
}
