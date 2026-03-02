#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::blockchain::error::BlockchainError;
use crate::core::hash::Hash32;
use crate::rpc::types::{
    GetBalanceParams, GetBalanceResult, GetChainResult, JsonRpcErrorObject, JsonRpcRequest,
    JsonRpcResponse, MineBlockParams, MineBlockResult, RpcState, SendTransactionParams,
    SendTransactionResult,
};

pub async fn handle_rpc(
    State(state): State<RpcState>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    Json(dispatch_request(state, request).await)
}

async fn dispatch_request(state: RpcState, request: JsonRpcRequest) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return JsonRpcResponse::error(
            request.id,
            -32600,
            "invalid request: jsonrpc must be '2.0'",
            None,
        );
    }

    match request.method.as_str() {
        "get_chain" => method_get_chain(state, request.id).await,
        "send_transaction" => method_send_transaction(state, request.id, request.params).await,
        "get_balance" => method_get_balance(state, request.id, request.params).await,
        "mine_block" => method_mine_block(state, request.id, request.params).await,
        _ => JsonRpcResponse::error(request.id, -32601, "method not found", None),
    }
}

async fn method_get_chain(state: RpcState, id: Option<Value>) -> JsonRpcResponse {
    let chain = state.lock().await;
    let result = GetChainResult {
        height: chain.chain_height(),
        tip_hash: hash_to_hex(&chain.tip_hash()),
        blocks: chain.blocks(),
    };
    JsonRpcResponse::success(id, json!(result))
}

async fn method_send_transaction(
    state: RpcState,
    id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params: SendTransactionParams = match parse_params(params) {
        Ok(params) => params,
        Err(error_object) => return response_with_error_object(id, error_object),
    };

    let mut chain = state.lock().await;
    match chain.admit_transaction(params.tx) {
        Ok(tx_hash) => JsonRpcResponse::success(
            id,
            json!(SendTransactionResult {
                tx_hash: hash_to_hex(&tx_hash),
            }),
        ),
        Err(error) => map_blockchain_error(id, error),
    }
}

async fn method_get_balance(
    state: RpcState,
    id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params: GetBalanceParams = match parse_params(params) {
        Ok(params) => params,
        Err(error_object) => return response_with_error_object(id, error_object),
    };

    let chain = state.lock().await;
    let result = GetBalanceResult {
        address: params.address.clone(),
        balance: chain.get_balance(&params.address),
    };
    JsonRpcResponse::success(id, json!(result))
}

async fn method_mine_block(
    state: RpcState,
    id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params: MineBlockParams = match parse_params_with_default(params) {
        Ok(params) => params,
        Err(error_object) => return response_with_error_object(id, error_object),
    };

    let timestamp = params.timestamp_unix.unwrap_or_else(current_unix_timestamp);
    let max_nonce = params.max_nonce.unwrap_or(1_000_000);

    let mut chain = state.lock().await;
    match chain.mine_next_block(timestamp, max_nonce) {
        Ok(block_hash) => JsonRpcResponse::success(
            id,
            json!(MineBlockResult {
                block_hash: hash_to_hex(&block_hash),
                height: chain.chain_height(),
            }),
        ),
        Err(error) => map_blockchain_error(id, error),
    }
}

fn map_blockchain_error(id: Option<Value>, error: BlockchainError) -> JsonRpcResponse {
    let (code, message) = match &error {
        BlockchainError::InvalidSignature { .. }
        | BlockchainError::InvalidSignatureEncoding { .. }
        | BlockchainError::UnknownSender { .. }
        | BlockchainError::SenderKeyMismatch { .. }
        | BlockchainError::InvalidNonce { .. }
        | BlockchainError::InsufficientBalance { .. }
        | BlockchainError::DuplicateMempoolTransaction => (-32010, error.to_string()),
        BlockchainError::MiningExhausted(_) => (-32020, error.to_string()),
        _ => (-32000, error.to_string()),
    };

    JsonRpcResponse::error(
        id,
        code,
        message,
        Some(json!({
            "error_kind": format!("{error:?}")
        })),
    )
}

fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, JsonRpcErrorObject> {
    let value = params.ok_or_else(|| JsonRpcErrorObject {
        code: -32602,
        message: "missing params".to_string(),
        data: None,
    })?;
    serde_json::from_value(value).map_err(|error| JsonRpcErrorObject {
        code: -32602,
        message: "invalid params".to_string(),
        data: Some(json!({ "details": error.to_string() })),
    })
}

fn parse_params_with_default<T: DeserializeOwned + Default>(
    params: Option<Value>,
) -> Result<T, JsonRpcErrorObject> {
    match params {
        Some(value) => serde_json::from_value(value).map_err(|error| JsonRpcErrorObject {
            code: -32602,
            message: "invalid params".to_string(),
            data: Some(json!({ "details": error.to_string() })),
        }),
        None => Ok(T::default()),
    }
}

fn response_with_error_object(id: Option<Value>, error: JsonRpcErrorObject) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(error),
    }
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn hash_to_hex(hash: &Hash32) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in &hash.0 {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use serde_json::{json, Value};
    use tempfile::tempdir;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    use crate::blockchain::chain::{Blockchain, ChainConfig};
    use crate::blockchain::state::GenesisAccount;
    use crate::core::transaction::{SignedTransactionPayload, Transaction};
    use crate::crypto::signature::SecretKeyBytes;
    use crate::crypto::wallet::Wallet;
    use crate::rpc::server::build_router;
    use crate::rpc::types::{JsonRpcResponse, RpcState};

    fn chain_config() -> ChainConfig {
        ChainConfig {
            difficulty_bits: 0,
            max_transactions_per_block: 1_000,
            genesis_timestamp_unix: 1_700_020_000,
        }
    }

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

    fn setup_state() -> Result<(RpcState, Wallet, Wallet, tempfile::TempDir), String> {
        let wallet_a = Wallet::from_secret_key(SecretKeyBytes([71u8; 32]));
        let wallet_b = Wallet::from_secret_key(SecretKeyBytes([72u8; 32]));
        let genesis = vec![
            GenesisAccount::from_public_key(&wallet_a.public_key_bytes(), 10_000),
            GenesisAccount::from_public_key(&wallet_b.public_key_bytes(), 500),
        ];

        let dir = tempdir().map_err(|error| error.to_string())?;
        let chain = Blockchain::open_or_init(dir.path(), chain_config(), genesis)
            .map_err(|error| error.to_string())?;
        let state = Arc::new(Mutex::new(chain));
        Ok((state, wallet_a, wallet_b, dir))
    }

    async fn rpc_call(state: RpcState, payload: Value) -> Result<JsonRpcResponse, String> {
        let app = build_router(state);
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .map_err(|error| error.to_string())?;

        let response = app
            .oneshot(request)
            .await
            .map_err(|error| error.to_string())?;
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|error| error.to_string())?;
        serde_json::from_slice::<JsonRpcResponse>(&body).map_err(|error| error.to_string())
    }

    #[tokio::test]
    async fn rpc_methods_work_end_to_end() -> Result<(), String> {
        let (state, wallet_a, wallet_b, _dir) = setup_state()?;

        let chain_response = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":1,"method":"get_chain"}),
        )
        .await?;
        assert!(chain_response.error.is_none());
        assert!(chain_response.result.is_some());

        let tx = signed_tx(&wallet_a, wallet_b.address(), 25, 1, 1);
        let send_response = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":2,"method":"send_transaction","params":{"tx":tx}}),
        )
        .await?;
        assert!(send_response.error.is_none());
        assert!(send_response.result.is_some());

        let mine_response = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":3,"method":"mine_block","params":{"timestamp_unix":1700020100,"max_nonce":0}}),
        )
        .await?;
        assert!(mine_response.error.is_none());
        let mine_result = mine_response
            .result
            .ok_or_else(|| "missing mine result".to_string())?;
        let height = mine_result
            .get("height")
            .and_then(Value::as_u64)
            .ok_or_else(|| "missing mined height".to_string())?;
        assert_eq!(height, 1);

        let balance_response = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":4,"method":"get_balance","params":{"address":wallet_b.address()}}),
        )
        .await?;
        assert!(balance_response.error.is_none());
        let balance = balance_response
            .result
            .and_then(|value| value.get("balance").cloned())
            .and_then(|value| value.as_u64())
            .ok_or_else(|| "missing balance result".to_string())?;
        assert_eq!(balance, 525);

        Ok(())
    }

    #[tokio::test]
    async fn bad_request_paths_return_jsonrpc_errors() -> Result<(), String> {
        let (state, wallet_a, wallet_b, _dir) = setup_state()?;

        let invalid_version = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"1.0","id":"a","method":"get_chain"}),
        )
        .await?;
        assert_eq!(invalid_version.error.map(|e| e.code), Some(-32600));

        let method_not_found = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":"b","method":"unknown_method"}),
        )
        .await?;
        assert_eq!(method_not_found.error.map(|e| e.code), Some(-32601));

        let invalid_params = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":"c","method":"get_balance","params":{"bad":"field"}}),
        )
        .await?;
        assert_eq!(invalid_params.error.map(|e| e.code), Some(-32602));

        let mut bad_sig_tx = signed_tx(&wallet_a, wallet_b.address(), 9, 1, 1);
        bad_sig_tx.signature[0] ^= 0x55;
        let business_error = rpc_call(
            Arc::clone(&state),
            json!({"jsonrpc":"2.0","id":"d","method":"send_transaction","params":{"tx":bad_sig_tx}}),
        )
        .await?;
        assert_eq!(business_error.error.map(|e| e.code), Some(-32010));

        Ok(())
    }
}
