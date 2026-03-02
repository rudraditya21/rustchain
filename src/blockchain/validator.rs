#![allow(dead_code)]

use crate::blockchain::error::BlockchainError;
use crate::blockchain::state::{genesis_ledger, AccountState, GenesisAccount, LedgerState};
use crate::core::block::Block;
use crate::core::hash::Hash32;
use crate::core::transaction::Transaction;
use crate::crypto::signature::{
    derive_address, verify_transaction_payload, PublicKeyBytes, SignatureBytes, SIGNATURE_LENGTH,
};

pub fn validate_chain(
    blocks: &[Block],
    genesis_accounts: &[GenesisAccount],
    expected_difficulty_bits: u32,
) -> Result<LedgerState, BlockchainError> {
    validate_chain_iter(blocks.iter(), genesis_accounts, expected_difficulty_bits)
}

pub fn validate_chain_iter<'a, I>(
    blocks: I,
    genesis_accounts: &[GenesisAccount],
    expected_difficulty_bits: u32,
) -> Result<LedgerState, BlockchainError>
where
    I: IntoIterator<Item = &'a Block>,
{
    let mut iter = blocks.into_iter();
    let genesis = iter.next().ok_or(BlockchainError::EmptyChain)?;
    validate_genesis_block(genesis)?;

    let mut ledger = genesis_ledger(genesis_accounts);
    let mut previous_hash = genesis.hash();

    for (offset, block) in iter.enumerate() {
        let height = (offset + 1) as u64;
        validate_block_header(block, height, &previous_hash, expected_difficulty_bits)?;
        validate_and_apply_block_transactions(block, height, &mut ledger)?;
        previous_hash = block.hash();
    }

    Ok(ledger)
}

pub fn validate_candidate_transactions(
    transactions: &[Transaction],
    ledger: &mut LedgerState,
) -> Result<(), BlockchainError> {
    for (index, tx) in transactions.iter().enumerate() {
        validate_and_apply_transaction(tx, ledger, u64::MAX, index)?;
    }
    Ok(())
}

pub fn validate_block_header(
    block: &Block,
    height: u64,
    expected_previous_hash: &Hash32,
    expected_difficulty_bits: u32,
) -> Result<(), BlockchainError> {
    if block.header.previous_block_hash != *expected_previous_hash {
        return Err(BlockchainError::InvalidPreviousHash { height });
    }

    if block.header.difficulty_bits != expected_difficulty_bits {
        return Err(BlockchainError::DifficultyMismatch {
            height,
            expected: expected_difficulty_bits,
            found: block.header.difficulty_bits,
        });
    }

    if !block.has_valid_merkle_root() {
        return Err(BlockchainError::InvalidMerkleRoot { height });
    }

    if !block.header.meets_pow_difficulty()? {
        return Err(BlockchainError::InvalidPow { height });
    }

    Ok(())
}

pub fn validate_and_apply_block_transactions(
    block: &Block,
    height: u64,
    ledger: &mut LedgerState,
) -> Result<(), BlockchainError> {
    for (index, tx) in block.transactions.iter().enumerate() {
        validate_and_apply_transaction(tx, ledger, height, index)?;
    }
    Ok(())
}

pub fn validate_and_apply_transaction(
    tx: &Transaction,
    ledger: &mut LedgerState,
    height: u64,
    tx_index: usize,
) -> Result<(), BlockchainError> {
    let (sender_address, sender_pubkey_hex, sender_public_key, signature) =
        parse_sender_key_and_signature(tx, height, tx_index)?;

    let payload = tx.signing_payload();
    if !verify_transaction_payload(&sender_public_key, &payload, &signature)? {
        return Err(BlockchainError::InvalidSignature { height, tx_index });
    }

    let sender_state = ledger
        .get_mut(&sender_address)
        .ok_or(BlockchainError::UnknownSender { height, tx_index })?;

    if let Some(existing) = &sender_state.public_key_hex {
        if existing != &sender_pubkey_hex {
            return Err(BlockchainError::SenderKeyMismatch { height, tx_index });
        }
    } else {
        sender_state.public_key_hex = Some(sender_pubkey_hex.clone());
    }

    let expected_nonce = sender_state.nonce + 1;
    if tx.nonce != expected_nonce {
        return Err(BlockchainError::InvalidNonce {
            height,
            tx_index,
            expected: expected_nonce,
            found: tx.nonce,
        });
    }

    let required = tx
        .amount
        .checked_add(tx.fee)
        .ok_or_else(|| BlockchainError::Serialization("amount overflow".to_string()))?;
    if sender_state.balance < required {
        return Err(BlockchainError::InsufficientBalance {
            height,
            tx_index,
            balance: sender_state.balance,
            required,
        });
    }

    sender_state.balance -= required;
    sender_state.nonce = tx.nonce;

    let recipient = ledger.entry(tx.to.clone()).or_insert(AccountState {
        balance: 0,
        nonce: 0,
        public_key_hex: None,
    });
    recipient.balance = recipient
        .balance
        .checked_add(tx.amount)
        .ok_or_else(|| BlockchainError::Serialization("recipient overflow".to_string()))?;

    Ok(())
}

fn parse_sender_key_and_signature(
    tx: &Transaction,
    height: u64,
    tx_index: usize,
) -> Result<(String, String, PublicKeyBytes, SignatureBytes), BlockchainError> {
    if tx.signature.len() != SIGNATURE_LENGTH {
        return Err(BlockchainError::InvalidSignatureEncoding { height, tx_index });
    }

    let mut signature = [0u8; SIGNATURE_LENGTH];
    signature.copy_from_slice(&tx.signature);

    let sender_public_key = PublicKeyBytes::from_hex(&tx.from)
        .map_err(|_| BlockchainError::InvalidSignature { height, tx_index })?;
    let sender_pubkey_hex = sender_public_key.to_hex();
    let sender_address = derive_address(&sender_public_key);

    Ok((
        sender_address,
        sender_pubkey_hex,
        sender_public_key,
        SignatureBytes(signature),
    ))
}

fn validate_genesis_block(genesis: &Block) -> Result<(), BlockchainError> {
    if genesis.header.previous_block_hash != Hash32::ZERO {
        return Err(BlockchainError::InvalidGenesis);
    }

    if !genesis.transactions.is_empty() {
        return Err(BlockchainError::InvalidGenesis);
    }

    if !genesis.has_valid_merkle_root() {
        return Err(BlockchainError::InvalidGenesis);
    }

    if !genesis.header.meets_pow_difficulty()? {
        return Err(BlockchainError::InvalidGenesis);
    }

    Ok(())
}
