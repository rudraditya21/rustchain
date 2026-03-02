#![allow(dead_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("corrupted entry in {namespace} for key {key}: {reason}")]
    CorruptedEntry {
        namespace: &'static str,
        key: String,
        reason: String,
    },
}
