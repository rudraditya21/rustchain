#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::core::block::Block;
use crate::core::transaction::Transaction;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolMessage {
    NewBlock { block: Block },
    NewTransaction { tx: Transaction },
    GetChain,
    ChainResponse { blocks: Vec<Block> },
}
