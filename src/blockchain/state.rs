#![allow(dead_code)]

use std::collections::HashMap;

use crate::crypto::signature::{derive_address, PublicKeyBytes};
use crate::storage::schema::AccountSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountState {
    pub balance: u64,
    pub nonce: u64,
    pub public_key_hex: Option<String>,
}

pub type LedgerState = HashMap<String, AccountState>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisAccount {
    pub address: String,
    pub public_key_hex: String,
    pub balance: u64,
}

impl GenesisAccount {
    pub fn from_public_key(public_key: &PublicKeyBytes, balance: u64) -> Self {
        Self {
            address: derive_address(public_key),
            public_key_hex: public_key.to_hex(),
            balance,
        }
    }
}

impl From<&AccountState> for AccountSnapshot {
    fn from(value: &AccountState) -> Self {
        Self {
            balance: value.balance,
            nonce: value.nonce,
            public_key_hex: value.public_key_hex.clone(),
        }
    }
}

pub fn genesis_ledger(genesis_accounts: &[GenesisAccount]) -> LedgerState {
    let mut ledger = LedgerState::new();
    for account in genesis_accounts {
        ledger.insert(
            account.address.clone(),
            AccountState {
                balance: account.balance,
                nonce: 0,
                public_key_hex: Some(account.public_key_hex.clone()),
            },
        );
    }
    ledger
}
