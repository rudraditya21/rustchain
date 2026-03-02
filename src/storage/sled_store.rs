#![allow(dead_code)]

use std::path::Path;

use serde::Serialize;

use crate::core::block::Block;
use crate::core::hash::Hash32;
use crate::core::transaction::Transaction;
use crate::storage::error::StorageError;
use crate::storage::schema::{
    key_account_snapshot, key_block_by_hash, key_height_to_hash, key_mempool, key_tip,
    prefix_mempool, AccountSnapshot, TipState, NS_ACCOUNT_SNAPSHOT, NS_BLOCK_BY_HASH,
    NS_HEIGHT_TO_HASH, NS_MEMPOOL, NS_TIP, STORAGE_TREE_NAME,
};

pub struct SledStore {
    db: sled::Db,
    state: sled::Tree,
}

impl SledStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let db = sled::open(path)?;
        let state = db.open_tree(STORAGE_TREE_NAME)?;
        Ok(Self { db, state })
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        self.db.flush()?;
        Ok(())
    }

    pub fn put_block(&self, height: u64, block: &Block) -> Result<Hash32, StorageError> {
        let block_hash = block.hash();
        let block_key = key_block_by_hash(&block_hash);
        let height_key = key_height_to_hash(height);
        let tip_key = key_tip();

        let block_value = serialize_json(block)?;
        let tip_value = serialize_json(&TipState { height, block_hash })?;

        let mut batch = sled::Batch::default();
        batch.insert(block_key, block_value);
        batch.insert(height_key, block_hash.0.to_vec());
        batch.insert(tip_key, tip_value);
        self.state.apply_batch(batch)?;

        Ok(block_hash)
    }

    pub fn get_block(&self, hash: &Hash32) -> Result<Option<Block>, StorageError> {
        let key = key_block_by_hash(hash);
        let Some(value) = self.state.get(&key)? else {
            return Ok(None);
        };

        let block = deserialize_json(NS_BLOCK_BY_HASH, &key, &value)?;
        Ok(Some(block))
    }

    pub fn get_hash_by_height(&self, height: u64) -> Result<Option<Hash32>, StorageError> {
        let key = key_height_to_hash(height);
        let Some(value) = self.state.get(&key)? else {
            return Ok(None);
        };

        if value.len() != 32 {
            return Err(StorageError::CorruptedEntry {
                namespace: NS_HEIGHT_TO_HASH,
                key: encode_hex(&key),
                reason: format!("expected 32-byte hash, found {} bytes", value.len()),
            });
        }

        let mut out = [0u8; 32];
        out.copy_from_slice(value.as_ref());
        Ok(Some(Hash32(out)))
    }

    pub fn load_tip(&self) -> Result<Option<TipState>, StorageError> {
        let key = key_tip();
        let Some(value) = self.state.get(key)? else {
            return Ok(None);
        };

        let tip = deserialize_json(NS_TIP, key, &value)?;
        Ok(Some(tip))
    }

    pub fn put_mempool_tx(&self, tx: &Transaction) -> Result<Hash32, StorageError> {
        let tx_hash = tx.tx_hash();
        let key = key_mempool(&tx_hash);
        let value = serialize_json(tx)?;
        self.state.insert(key, value)?;
        Ok(tx_hash)
    }

    pub fn get_mempool_tx(&self, tx_hash: &Hash32) -> Result<Option<Transaction>, StorageError> {
        let key = key_mempool(tx_hash);
        let Some(value) = self.state.get(&key)? else {
            return Ok(None);
        };

        let tx = deserialize_json(NS_MEMPOOL, &key, &value)?;
        Ok(Some(tx))
    }

    pub fn list_mempool_txs(&self) -> Result<Vec<Transaction>, StorageError> {
        let mut transactions = Vec::new();

        for item in self.state.scan_prefix(prefix_mempool()) {
            let (key, value) = item?;
            let tx = deserialize_json(NS_MEMPOOL, key.as_ref(), value.as_ref())?;
            transactions.push(tx);
        }

        Ok(transactions)
    }

    pub fn remove_mempool_tx(&self, tx_hash: &Hash32) -> Result<bool, StorageError> {
        let key = key_mempool(tx_hash);
        let removed = self.state.remove(key)?;
        Ok(removed.is_some())
    }

    pub fn clear_mempool(&self) -> Result<usize, StorageError> {
        let mut keys = Vec::new();
        for item in self.state.scan_prefix(prefix_mempool()) {
            let (key, _) = item?;
            keys.push(key.to_vec());
        }

        let removed = keys.len();
        let mut batch = sled::Batch::default();
        for key in keys {
            batch.remove(key);
        }
        self.state.apply_batch(batch)?;
        Ok(removed)
    }

    pub fn put_account_snapshot(
        &self,
        address: &str,
        snapshot: &AccountSnapshot,
    ) -> Result<(), StorageError> {
        let key = key_account_snapshot(address);
        let value = serialize_json(snapshot)?;
        self.state.insert(key, value)?;
        Ok(())
    }

    pub fn get_account_snapshot(
        &self,
        address: &str,
    ) -> Result<Option<AccountSnapshot>, StorageError> {
        let key = key_account_snapshot(address);
        let Some(value) = self.state.get(&key)? else {
            return Ok(None);
        };

        let snapshot = deserialize_json(NS_ACCOUNT_SNAPSHOT, &key, &value)?;
        Ok(Some(snapshot))
    }
}

fn serialize_json<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    serde_json::to_vec(value).map_err(|error| StorageError::Serialization(error.to_string()))
}

fn deserialize_json<T: serde::de::DeserializeOwned>(
    namespace: &'static str,
    key: &[u8],
    bytes: &[u8],
) -> Result<T, StorageError> {
    serde_json::from_slice(bytes).map_err(|error| StorageError::CorruptedEntry {
        namespace,
        key: encode_hex(key),
        reason: error.to_string(),
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::core::block::{Block, BlockHeader};
    use crate::core::hash::Hash32;
    use crate::core::merkle::MerkleTree;
    use crate::core::transaction::Transaction;
    use crate::storage::error::StorageError;
    use crate::storage::schema::{key_height_to_hash, key_mempool, NS_HEIGHT_TO_HASH, NS_MEMPOOL};
    use crate::storage::sled_store::SledStore;

    fn sample_tx(nonce: u64) -> Transaction {
        Transaction {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 10 + nonce,
            fee: 1,
            nonce,
            signature: vec![1, 2, 3, 4],
        }
    }

    fn sample_block() -> Block {
        let transactions = vec![sample_tx(1), sample_tx(2)];
        let merkle_root = MerkleTree::from_transactions(&transactions).root();

        Block {
            header: BlockHeader {
                version: 1,
                previous_block_hash: Hash32([0u8; 32]),
                merkle_root,
                timestamp_unix: 1_700_000_111,
                nonce: 0,
                difficulty_bits: 0,
            },
            transactions,
        }
    }

    #[test]
    fn persistence_across_restart_restores_chain_and_mempool() -> Result<(), StorageError> {
        let dir = tempdir()?;
        let db_path = dir.path().join("node.sled");
        let expected_block = sample_block();
        let expected_tx = sample_tx(99);

        let stored_hash = {
            let store = SledStore::open(&db_path)?;
            let hash = store.put_block(1, &expected_block)?;
            store.put_mempool_tx(&expected_tx)?;
            store.flush()?;
            hash
        };

        let store = SledStore::open(&db_path)?;
        let recovered_tip = store
            .load_tip()?
            .ok_or_else(|| StorageError::Serialization("missing tip after restart".to_string()))?;
        assert_eq!(recovered_tip.height, 1);
        assert_eq!(recovered_tip.block_hash, stored_hash);

        let recovered_height_hash = store.get_hash_by_height(1)?.ok_or_else(|| {
            StorageError::Serialization("missing height_to_hash entry after restart".to_string())
        })?;
        assert_eq!(recovered_height_hash, stored_hash);

        let recovered_block = store.get_block(&stored_hash)?.ok_or_else(|| {
            StorageError::Serialization("missing block after restart".to_string())
        })?;
        assert_eq!(recovered_block, expected_block);

        let mempool = store.list_mempool_txs()?;
        assert_eq!(mempool.len(), 1);
        assert_eq!(mempool[0], expected_tx);
        Ok(())
    }

    #[test]
    fn tip_recovery_after_reopen() -> Result<(), StorageError> {
        let dir = tempdir()?;
        let db_path = dir.path().join("tip.sled");
        let block = sample_block();

        let expected_hash = {
            let store = SledStore::open(&db_path)?;
            let hash = store.put_block(42, &block)?;
            store.flush()?;
            hash
        };

        let store = SledStore::open(&db_path)?;
        let tip = store
            .load_tip()?
            .ok_or_else(|| StorageError::Serialization("missing tip".to_string()))?;
        assert_eq!(tip.height, 42);
        assert_eq!(tip.block_hash, expected_hash);
        Ok(())
    }

    #[test]
    fn mempool_crud_roundtrip() -> Result<(), StorageError> {
        let dir = tempdir()?;
        let db_path = dir.path().join("mempool.sled");
        let store = SledStore::open(&db_path)?;

        let tx_a = sample_tx(10);
        let tx_b = sample_tx(11);
        let tx_a_hash = store.put_mempool_tx(&tx_a)?;
        let tx_b_hash = store.put_mempool_tx(&tx_b)?;

        let fetched = store.get_mempool_tx(&tx_a_hash)?.ok_or_else(|| {
            StorageError::Serialization("failed to fetch tx_a from mempool".to_string())
        })?;
        assert_eq!(fetched, tx_a);

        let all = store.list_mempool_txs()?;
        assert_eq!(all.len(), 2);

        assert!(store.remove_mempool_tx(&tx_b_hash)?);
        assert!(!store.remove_mempool_tx(&tx_b_hash)?);

        let removed_count = store.clear_mempool()?;
        assert_eq!(removed_count, 1);
        assert!(store.list_mempool_txs()?.is_empty());
        Ok(())
    }

    #[test]
    fn corrupted_entry_handling_detects_invalid_values() -> Result<(), StorageError> {
        let dir = tempdir()?;
        let db_path = dir.path().join("corrupt.sled");
        let store = SledStore::open(&db_path)?;

        let bad_height_key = key_height_to_hash(7);
        store
            .state
            .insert(&bad_height_key, b"short_hash".as_slice())?;
        let height_err = store.get_hash_by_height(7);
        assert!(matches!(
            height_err,
            Err(StorageError::CorruptedEntry {
                namespace: NS_HEIGHT_TO_HASH,
                ..
            })
        ));

        let bad_tx_hash = Hash32([9u8; 32]);
        let bad_mempool_key = key_mempool(&bad_tx_hash);
        store
            .state
            .insert(&bad_mempool_key, b"{not_json".as_slice())?;
        let mempool_err = store.get_mempool_tx(&bad_tx_hash);
        assert!(matches!(
            mempool_err,
            Err(StorageError::CorruptedEntry {
                namespace: NS_MEMPOOL,
                ..
            })
        ));

        Ok(())
    }
}
