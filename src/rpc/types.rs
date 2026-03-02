#![allow(dead_code)]

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::blockchain::chain::Blockchain;
use crate::core::block::Block;
use crate::core::transaction::Transaction;

pub type RpcState = Arc<Mutex<Blockchain>>;

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendTransactionParams {
    pub tx: Transaction,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetBalanceParams {
    pub address: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MineBlockParams {
    pub timestamp_unix: Option<u64>,
    pub max_nonce: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetChainResult {
    pub height: u64,
    pub tip_hash: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionResult {
    pub tx_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceResult {
    pub address: String,
    pub balance: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineBlockResult {
    pub block_hash: String,
    pub height: u64,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(
        id: Option<Value>,
        code: i32,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcErrorObject {
                code,
                message: message.into(),
                data,
            }),
        }
    }
}
