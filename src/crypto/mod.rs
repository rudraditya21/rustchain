#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyBytes(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureBytes(pub [u8; 64]);
