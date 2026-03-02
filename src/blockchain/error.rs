#![allow(dead_code)]

use thiserror::Error;

use crate::core::error::CoreError;
use crate::crypto::error::CryptoError;
use crate::storage::error::StorageError;

#[derive(Debug, Error)]
pub enum BlockchainError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("core error: {0}")]
    Core(#[from] CoreError),

    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("corrupted chain at height {0}")]
    CorruptedChain(u64),

    #[error("chain is empty")]
    EmptyChain,

    #[error("invalid genesis block")]
    InvalidGenesis,

    #[error("invalid previous hash at height {height}")]
    InvalidPreviousHash { height: u64 },

    #[error("invalid merkle root at height {height}")]
    InvalidMerkleRoot { height: u64 },

    #[error("invalid proof of work at height {height}")]
    InvalidPow { height: u64 },

    #[error("unexpected difficulty at height {height}: expected {expected}, found {found}")]
    DifficultyMismatch {
        height: u64,
        expected: u32,
        found: u32,
    },

    #[error("invalid signature at height {height}, tx index {tx_index}")]
    InvalidSignature { height: u64, tx_index: usize },

    #[error("invalid signature encoding at height {height}, tx index {tx_index}")]
    InvalidSignatureEncoding { height: u64, tx_index: usize },

    #[error("unknown sender at height {height}, tx index {tx_index}")]
    UnknownSender { height: u64, tx_index: usize },

    #[error("sender key mismatch at height {height}, tx index {tx_index}")]
    SenderKeyMismatch { height: u64, tx_index: usize },

    #[error(
        "invalid nonce at height {height}, tx index {tx_index}: expected {expected}, found {found}"
    )]
    InvalidNonce {
        height: u64,
        tx_index: usize,
        expected: u64,
        found: u64,
    },

    #[error("insufficient balance at height {height}, tx index {tx_index}: balance {balance}, required {required}")]
    InsufficientBalance {
        height: u64,
        tx_index: usize,
        balance: u64,
        required: u64,
    },

    #[error("duplicate mempool transaction")]
    DuplicateMempoolTransaction,

    #[error("failed to find pow nonce within limit {0}")]
    MiningExhausted(u64),

    #[error("serialization error: {0}")]
    Serialization(String),
}
