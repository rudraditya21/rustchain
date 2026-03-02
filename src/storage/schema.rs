#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::core::hash::Hash32;

pub const STORAGE_TREE_NAME: &str = "state";

pub const NS_BLOCK_BY_HASH: &str = "block_by_hash";
pub const NS_HEIGHT_TO_HASH: &str = "height_to_hash";
pub const NS_TIP: &str = "tip";
pub const NS_MEMPOOL: &str = "mempool";
pub const NS_ACCOUNT_SNAPSHOT: &str = "account_snapshot";

const KEY_BLOCK_BY_HASH_PREFIX: &[u8] = b"block_by_hash/";
const KEY_HEIGHT_TO_HASH_PREFIX: &[u8] = b"height_to_hash/";
const KEY_MEMPOOL_PREFIX: &[u8] = b"mempool/";
const KEY_ACCOUNT_SNAPSHOT_PREFIX: &[u8] = b"account_snapshot/";
const KEY_TIP: &[u8] = b"tip";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TipState {
    pub height: u64,
    pub block_hash: Hash32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub balance: u64,
    pub nonce: u64,
}

pub fn key_block_by_hash(hash: &Hash32) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_BLOCK_BY_HASH_PREFIX.len() + hash.0.len());
    key.extend_from_slice(KEY_BLOCK_BY_HASH_PREFIX);
    key.extend_from_slice(&hash.0);
    key
}

pub fn key_height_to_hash(height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_HEIGHT_TO_HASH_PREFIX.len() + std::mem::size_of::<u64>());
    key.extend_from_slice(KEY_HEIGHT_TO_HASH_PREFIX);
    key.extend_from_slice(&height.to_be_bytes());
    key
}

pub fn key_tip() -> &'static [u8] {
    KEY_TIP
}

pub fn key_mempool(tx_hash: &Hash32) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_MEMPOOL_PREFIX.len() + tx_hash.0.len());
    key.extend_from_slice(KEY_MEMPOOL_PREFIX);
    key.extend_from_slice(&tx_hash.0);
    key
}

pub fn key_account_snapshot(address: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_ACCOUNT_SNAPSHOT_PREFIX.len() + address.len());
    key.extend_from_slice(KEY_ACCOUNT_SNAPSHOT_PREFIX);
    key.extend_from_slice(address.as_bytes());
    key
}

pub fn prefix_mempool() -> &'static [u8] {
    KEY_MEMPOOL_PREFIX
}
