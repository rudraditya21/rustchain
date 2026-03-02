use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("configuration parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("configuration file does not exist: {0}")]
    ConfigNotFound(PathBuf),

    #[error("logging initialization failed: {0}")]
    LoggingInit(String),

    #[error("core error: {0}")]
    Core(#[from] crate::core::error::CoreError),

    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::error::CryptoError),

    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::error::StorageError),

    #[error("blockchain error: {0}")]
    Blockchain(#[from] crate::blockchain::error::BlockchainError),

    #[error("network error: {0}")]
    Network(#[from] crate::network::error::NetworkError),

    #[error("serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("RPC endpoint returned HTTP status {0}")]
    RpcHttpStatus(u16),

    #[error("RPC error {code}: {message}")]
    Rpc { code: i32, message: String },

    #[error("invalid RPC URL: {0}")]
    InvalidRpcUrl(String),

    #[error("invalid RPC response: {0}")]
    InvalidRpcResponse(String),

    #[error("wallet file error for {path}: {reason}")]
    WalletFile { path: PathBuf, reason: String },
}

#[cfg(test)]
mod tests {
    use super::AppError;
    use crate::blockchain::error::BlockchainError;

    #[test]
    fn io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let app_error: AppError = io_error.into();

        assert!(matches!(app_error, AppError::Io(_)));
    }

    #[test]
    fn toml_error_conversion() {
        let parse_result = toml::from_str::<crate::config::Config>("this is not valid toml");

        let parse_error = match parse_result {
            Ok(_) => panic!("expected TOML parse to fail"),
            Err(error) => error,
        };

        let app_error: AppError = parse_error.into();
        assert!(matches!(app_error, AppError::ConfigParse(_)));
    }

    #[test]
    fn blockchain_error_conversion() {
        let app_error: AppError = BlockchainError::EmptyChain.into();
        assert!(matches!(app_error, AppError::Blockchain(_)));
    }
}
