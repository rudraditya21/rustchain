#![allow(dead_code)]

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("invalid hex length for {field}: expected {expected}, got {found}")]
    HexLengthMismatch {
        field: &'static str,
        expected: usize,
        found: usize,
    },

    #[error("invalid hex character for {field} at index {index}: {value}")]
    HexCharacter {
        field: &'static str,
        index: usize,
        value: char,
    },

    #[error("invalid ed25519 public key bytes")]
    PublicKeyParse,
}
