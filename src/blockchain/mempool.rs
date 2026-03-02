#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::core::hash::Hash32;
use crate::core::transaction::Transaction;

#[derive(Debug, Default, Clone)]
pub struct Mempool {
    entries: BTreeMap<[u8; 32], Transaction>,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains_hash(&self, hash: &Hash32) -> bool {
        self.entries.contains_key(&hash.0)
    }

    pub fn insert(&mut self, tx: Transaction) -> Hash32 {
        let tx_hash = tx.tx_hash();
        self.entries.insert(tx_hash.0, tx);
        tx_hash
    }

    pub fn remove(&mut self, hash: &Hash32) -> Option<Transaction> {
        self.entries.remove(&hash.0)
    }

    pub fn ordered_transactions(&self) -> Vec<(Hash32, Transaction)> {
        self.entries
            .iter()
            .map(|(hash, tx)| (Hash32(*hash), tx.clone()))
            .collect()
    }
}
