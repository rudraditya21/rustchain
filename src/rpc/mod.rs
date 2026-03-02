#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcMethod {
    GetChain,
    SendTransaction,
    GetBalance,
    MineBlock,
}
