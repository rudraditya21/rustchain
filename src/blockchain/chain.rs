#![allow(dead_code)]

use std::path::Path;

use crate::blockchain::error::BlockchainError;
use crate::blockchain::mempool::Mempool;
use crate::blockchain::state::{genesis_ledger, GenesisAccount, LedgerState};
use crate::blockchain::validator::{
    validate_and_apply_block_transactions, validate_block_header, validate_candidate_transactions,
    validate_chain,
};
use crate::core::block::{Block, BlockHeader};
use crate::core::hash::Hash32;
use crate::core::merkle::MerkleTree;
use crate::core::transaction::Transaction;
use crate::storage::schema::AccountSnapshot;
use crate::storage::sled_store::SledStore;

#[derive(Debug, Clone, Copy)]
pub struct ChainConfig {
    pub difficulty_bits: u32,
    pub max_transactions_per_block: usize,
    pub genesis_timestamp_unix: u64,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            difficulty_bits: 8,
            max_transactions_per_block: 1_000,
            genesis_timestamp_unix: 1_700_000_000,
        }
    }
}

#[derive(Debug, Clone)]
struct ChainEntry {
    height: u64,
    hash: Hash32,
    block: Block,
}

pub struct Blockchain {
    store: SledStore,
    config: ChainConfig,
    genesis_accounts: Vec<GenesisAccount>,
    chain: Vec<ChainEntry>,
    ledger: LedgerState,
    mempool: Mempool,
}

impl Blockchain {
    pub fn open_or_init(
        db_path: impl AsRef<Path>,
        config: ChainConfig,
        genesis_accounts: Vec<GenesisAccount>,
    ) -> Result<Self, BlockchainError> {
        let store = SledStore::open(db_path)?;

        let mut chain = Self {
            store,
            config,
            genesis_accounts,
            chain: Vec::new(),
            ledger: LedgerState::new(),
            mempool: Mempool::new(),
        };

        chain.bootstrap()?;
        Ok(chain)
    }

    pub fn chain_height(&self) -> u64 {
        self.chain.last().map_or(0, |entry| entry.height)
    }

    pub fn tip_hash(&self) -> Hash32 {
        self.chain.last().map_or(Hash32::ZERO, |entry| entry.hash)
    }

    pub fn blocks(&self) -> Vec<Block> {
        self.chain.iter().map(|entry| entry.block.clone()).collect()
    }

    pub fn mempool_len(&self) -> usize {
        self.mempool.len()
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        self.ledger
            .get(address)
            .map_or(0, |account| account.balance)
    }

    pub fn admit_transaction(&mut self, tx: Transaction) -> Result<Hash32, BlockchainError> {
        let tx_hash = tx.tx_hash();
        if self.mempool.contains_hash(&tx_hash) {
            return Err(BlockchainError::DuplicateMempoolTransaction);
        }

        let mut projected = self.projected_ledger_with_mempool()?;
        validate_candidate_transactions(std::slice::from_ref(&tx), &mut projected)?;

        self.mempool.insert(tx.clone());
        self.store.put_mempool_tx(&tx)?;
        Ok(tx_hash)
    }

    pub fn build_candidate_block(&self, timestamp_unix: u64) -> Block {
        let mut transactions = self.mempool.ordered_transactions();
        if transactions.len() > self.config.max_transactions_per_block {
            transactions.truncate(self.config.max_transactions_per_block);
        }

        let txs: Vec<Transaction> = transactions.into_iter().map(|(_, tx)| tx).collect();
        let merkle_root = MerkleTree::from_transactions(&txs).root();

        Block {
            header: BlockHeader {
                version: 1,
                previous_block_hash: self.tip_hash(),
                merkle_root,
                timestamp_unix,
                nonce: 0,
                difficulty_bits: self.config.difficulty_bits,
            },
            transactions: txs,
        }
    }

    pub fn mine_candidate_block(
        &self,
        mut candidate: Block,
        max_nonce: u64,
    ) -> Result<Block, BlockchainError> {
        for nonce in 0..=max_nonce {
            candidate.header.nonce = nonce;
            if candidate.header.meets_pow_difficulty()? {
                return Ok(candidate);
            }
        }
        Err(BlockchainError::MiningExhausted(max_nonce))
    }

    pub fn mine_next_block(
        &mut self,
        timestamp_unix: u64,
        max_nonce: u64,
    ) -> Result<Hash32, BlockchainError> {
        let candidate = self.build_candidate_block(timestamp_unix);
        let mined = self.mine_candidate_block(candidate, max_nonce)?;
        self.apply_block(mined)
    }

    pub fn apply_block(&mut self, block: Block) -> Result<Hash32, BlockchainError> {
        let expected_height = self.chain_height() + 1;
        validate_block_header(
            &block,
            expected_height,
            &self.tip_hash(),
            self.config.difficulty_bits,
        )?;

        let mut next_ledger = self.ledger.clone();
        validate_and_apply_block_transactions(&block, expected_height, &mut next_ledger)?;

        let hash = block.hash();
        self.store.put_block(expected_height, &block)?;
        self.persist_ledger_snapshots(&next_ledger)?;

        for tx in &block.transactions {
            let tx_hash = tx.tx_hash();
            self.mempool.remove(&tx_hash);
            self.store.remove_mempool_tx(&tx_hash)?;
        }

        self.chain.push(ChainEntry {
            height: expected_height,
            hash,
            block,
        });
        self.ledger = next_ledger;

        Ok(hash)
    }

    pub fn validate_full_chain(&self) -> Result<(), BlockchainError> {
        let blocks = self.blocks();
        validate_chain(&blocks, &self.genesis_accounts, self.config.difficulty_bits)?;
        Ok(())
    }

    fn bootstrap(&mut self) -> Result<(), BlockchainError> {
        if self.store.load_tip()?.is_none() {
            self.initialize_genesis()?;
        }

        self.reload_chain_and_ledger()?;
        self.reload_mempool()?;
        Ok(())
    }

    fn initialize_genesis(&mut self) -> Result<(), BlockchainError> {
        let genesis = self.genesis_block();
        let hash = self.store.put_block(0, &genesis)?;

        let ledger = genesis_ledger(&self.genesis_accounts);
        self.persist_ledger_snapshots(&ledger)?;

        self.chain = vec![ChainEntry {
            height: 0,
            hash,
            block: genesis,
        }];
        self.ledger = ledger;
        Ok(())
    }

    fn genesis_block(&self) -> Block {
        let transactions = Vec::new();
        let merkle_root = MerkleTree::from_transactions(&transactions).root();
        Block {
            header: BlockHeader {
                version: 1,
                previous_block_hash: Hash32::ZERO,
                merkle_root,
                timestamp_unix: self.config.genesis_timestamp_unix,
                nonce: 0,
                difficulty_bits: 0,
            },
            transactions,
        }
    }

    fn reload_chain_and_ledger(&mut self) -> Result<(), BlockchainError> {
        let tip = self
            .store
            .load_tip()?
            .ok_or(BlockchainError::CorruptedChain(0))?;

        let mut entries = Vec::new();
        for height in 0..=tip.height {
            let hash = self
                .store
                .get_hash_by_height(height)?
                .ok_or(BlockchainError::CorruptedChain(height))?;
            let block = self
                .store
                .get_block(&hash)?
                .ok_or(BlockchainError::CorruptedChain(height))?;
            entries.push(ChainEntry {
                height,
                hash,
                block,
            });
        }

        let blocks: Vec<Block> = entries.iter().map(|entry| entry.block.clone()).collect();
        let ledger = validate_chain(&blocks, &self.genesis_accounts, self.config.difficulty_bits)?;

        self.chain = entries;
        self.ledger = ledger;
        Ok(())
    }

    fn reload_mempool(&mut self) -> Result<(), BlockchainError> {
        self.mempool = Mempool::new();

        let mut projected = self.ledger.clone();
        let mut txs = self.store.list_mempool_txs()?;
        txs.sort_by_key(Transaction::tx_hash);

        for tx in txs {
            if validate_candidate_transactions(std::slice::from_ref(&tx), &mut projected).is_ok() {
                self.mempool.insert(tx);
            } else {
                let tx_hash = tx.tx_hash();
                self.store.remove_mempool_tx(&tx_hash)?;
            }
        }

        Ok(())
    }

    fn persist_ledger_snapshots(&self, ledger: &LedgerState) -> Result<(), BlockchainError> {
        for (address, state) in ledger {
            let snapshot = AccountSnapshot::from(state);
            self.store.put_account_snapshot(address, &snapshot)?;
        }
        Ok(())
    }

    fn projected_ledger_with_mempool(&self) -> Result<LedgerState, BlockchainError> {
        let mut projected = self.ledger.clone();
        let ordered = self.mempool.ordered_transactions();
        let txs: Vec<Transaction> = ordered.into_iter().map(|(_, tx)| tx).collect();
        validate_candidate_transactions(&txs, &mut projected)?;
        Ok(projected)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseError;
    use tempfile::tempdir;

    use crate::blockchain::chain::{Blockchain, ChainConfig};
    use crate::blockchain::error::BlockchainError;
    use crate::blockchain::state::GenesisAccount;
    use crate::blockchain::validator::validate_chain;
    use crate::core::block::Block;
    use crate::core::hash::Hash32;
    use crate::core::transaction::{SignedTransactionPayload, Transaction};
    use crate::crypto::signature::SecretKeyBytes;
    use crate::crypto::wallet::Wallet;

    fn signed_tx(wallet: &Wallet, to: String, amount: u64, fee: u64, nonce: u64) -> Transaction {
        let payload = SignedTransactionPayload {
            from: wallet.public_key_hex(),
            to,
            amount,
            fee,
            nonce,
        };
        let signature = wallet.sign_payload(&payload);

        Transaction {
            from: payload.from,
            to: payload.to,
            amount,
            fee,
            nonce,
            signature: signature.0.to_vec(),
        }
    }

    fn wallets_and_genesis() -> (Wallet, Wallet, Vec<GenesisAccount>) {
        let wallet_a = Wallet::from_secret_key(SecretKeyBytes([11u8; 32]));
        let wallet_b = Wallet::from_secret_key(SecretKeyBytes([22u8; 32]));

        let genesis = vec![
            GenesisAccount::from_public_key(&wallet_a.public_key_bytes(), 10_000),
            GenesisAccount::from_public_key(&wallet_b.public_key_bytes(), 1_000),
        ];

        (wallet_a, wallet_b, genesis)
    }

    fn chain_config(difficulty_bits: u32) -> ChainConfig {
        ChainConfig {
            difficulty_bits,
            max_transactions_per_block: 1_000,
            genesis_timestamp_unix: 1_700_000_000,
        }
    }

    #[test]
    fn rejects_invalid_previous_hash() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(8), genesis)?;

        let tx = signed_tx(&wallet_a, wallet_b.address(), 5, 1, 1);
        chain.admit_transaction(tx)?;

        let mut block =
            chain.mine_candidate_block(chain.build_candidate_block(1_700_000_001), 1_000_000)?;
        block.header.previous_block_hash = Hash32([9u8; 32]);

        let result = chain.apply_block(block);
        assert!(matches!(
            result,
            Err(BlockchainError::InvalidPreviousHash { .. })
        ));
        Ok(())
    }

    #[test]
    fn rejects_bad_merkle_root() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(8), genesis)?;

        let tx = signed_tx(&wallet_a, wallet_b.address(), 5, 1, 1);
        chain.admit_transaction(tx)?;

        let mut block =
            chain.mine_candidate_block(chain.build_candidate_block(1_700_000_001), 1_000_000)?;
        block.header.merkle_root = Hash32([1u8; 32]);

        let result = chain.apply_block(block);
        assert!(matches!(
            result,
            Err(BlockchainError::InvalidMerkleRoot { .. })
        ));
        Ok(())
    }

    #[test]
    fn rejects_bad_pow() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(10), genesis)?;

        let tx = signed_tx(&wallet_a, wallet_b.address(), 5, 1, 1);
        chain.admit_transaction(tx)?;

        let mut block = chain.build_candidate_block(1_700_000_001);
        block.header.nonce = 0;
        let result = chain.apply_block(block);
        assert!(matches!(result, Err(BlockchainError::InvalidPow { .. })));
        Ok(())
    }

    #[test]
    fn rejects_bad_signature() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(0), genesis)?;

        let mut tx = signed_tx(&wallet_a, wallet_b.address(), 5, 1, 1);
        tx.signature[0] ^= 0xAA;

        let result = chain.admit_transaction(tx);
        assert!(matches!(
            result,
            Err(BlockchainError::InvalidSignature { .. })
        ));
        Ok(())
    }

    #[test]
    fn rejects_nonce_replay_in_mempool() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(0), genesis)?;

        let tx1 = signed_tx(&wallet_a, wallet_b.address(), 5, 1, 1);
        let tx2 = signed_tx(&wallet_a, wallet_b.address(), 6, 1, 1);

        chain.admit_transaction(tx1)?;
        let result = chain.admit_transaction(tx2);
        assert!(matches!(result, Err(BlockchainError::InvalidNonce { .. })));
        Ok(())
    }

    #[test]
    fn rejects_insufficient_funds() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(0), genesis)?;

        let tx = signed_tx(&wallet_a, wallet_b.address(), 1_000_000, 1, 1);
        let result = chain.admit_transaction(tx);
        assert!(matches!(
            result,
            Err(BlockchainError::InsufficientBalance { .. })
        ));
        Ok(())
    }

    #[test]
    fn full_chain_validation_is_deterministic() -> Result<(), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(0), genesis.clone())?;

        for nonce in 1..=3 {
            let tx = signed_tx(&wallet_a, wallet_b.address(), 3, 1, nonce);
            chain.admit_transaction(tx)?;
            chain.mine_next_block(1_700_000_100 + nonce, 0)?;
        }

        chain.validate_full_chain()?;
        chain.validate_full_chain()?;

        let blocks = chain.blocks();
        let ledger = validate_chain(&blocks, &genesis, 0)?;
        let sender = wallet_a.address();
        let receiver = wallet_b.address();
        assert_eq!(ledger.get(&sender).map_or(0, |a| a.nonce), 3);
        assert_eq!(ledger.get(&receiver).map_or(0, |a| a.balance), 1_009);
        Ok(())
    }

    fn build_valid_chain_with_amounts(
        amounts: &[u64],
    ) -> Result<(Vec<Block>, Vec<GenesisAccount>), BlockchainError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir = tempdir()?;
        let mut chain = Blockchain::open_or_init(dir.path(), chain_config(0), genesis.clone())?;

        for (index, amount) in amounts.iter().enumerate() {
            let nonce = (index + 1) as u64;
            let tx = signed_tx(&wallet_a, wallet_b.address(), *amount, 1, nonce);
            chain.admit_transaction(tx)?;
            chain.mine_next_block(1_700_000_500 + nonce, 0)?;
        }

        Ok((chain.blocks(), genesis))
    }

    proptest! {
        #[test]
        fn prop_valid_chain_survives_serde_roundtrip(amounts in prop::collection::vec(1u64..30u64, 1..6)) {
            let (blocks, genesis) = build_valid_chain_with_amounts(&amounts)
                .map_err(|error| TestCaseError::fail(format!("{error}")))?;

            let encoded = serde_json::to_vec(&blocks)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let decoded: Vec<Block> = serde_json::from_slice(&encoded)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;

            let validated = validate_chain(&decoded, &genesis, 0);
            prop_assert!(validated.is_ok());
        }

        #[test]
        fn prop_random_mutation_gets_rejected(
            amounts in prop::collection::vec(1u64..30u64, 1..6),
            mutate_kind in 0u8..3u8
        ) {
            let (mut blocks, genesis) = build_valid_chain_with_amounts(&amounts)
                .map_err(|error| TestCaseError::fail(format!("{error}")))?;

            let target_index = if blocks.len() > 1 { 1 } else { 0 };
            match mutate_kind {
                0 => {
                    blocks[target_index].header.merkle_root.0[0] ^= 1;
                }
                1 => {
                    if target_index > 0 {
                        blocks[target_index].header.previous_block_hash.0[0] ^= 1;
                    } else {
                        blocks[target_index].header.merkle_root.0[1] ^= 1;
                    }
                }
                _ => {
                    if let Some(tx) = blocks[target_index].transactions.get_mut(0) {
                        tx.amount = tx.amount.saturating_add(1);
                    } else {
                        blocks[target_index].header.merkle_root.0[2] ^= 1;
                    }
                }
            }

            let validated = validate_chain(&blocks, &genesis, 0);
            prop_assert!(validated.is_err());
        }
    }
}
