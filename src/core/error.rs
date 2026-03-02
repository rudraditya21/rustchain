#![allow(dead_code)]

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error(
        "unexpected end of input while decoding {field}: needed {needed} bytes, found {remaining}"
    )]
    UnexpectedEof {
        field: &'static str,
        needed: usize,
        remaining: usize,
    },

    #[error("invalid UTF-8 for field: {0}")]
    InvalidUtf8(&'static str),

    #[error("trailing bytes after decode: {0}")]
    TrailingBytes(usize),

    #[error("invalid encoding version for {field}: expected {expected}, got {found}")]
    InvalidEncodingVersion {
        field: &'static str,
        expected: u8,
        found: u8,
    },

    #[error("difficulty bits must be <= 256, got {0}")]
    InvalidDifficulty(u32),
}
