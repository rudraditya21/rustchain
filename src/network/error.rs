#![allow(dead_code)]

use thiserror::Error;

use crate::blockchain::error::BlockchainError;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("address parse error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("blockchain error: {0}")]
    Blockchain(#[from] BlockchainError),

    #[error("frame too large: {0}")]
    FrameTooLarge(usize),

    #[error("unexpected protocol message: {0}")]
    UnexpectedMessage(&'static str),
}
