#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkMessage {
    NewBlock,
    NewTransaction,
    GetChain,
    ChainResponse,
}
