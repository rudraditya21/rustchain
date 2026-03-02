#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleTree {
    pub root: [u8; 32],
}
