use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex};

use crate::blockchain::chain::{Blockchain, ChainConfig};
use crate::blockchain::state::GenesisAccount;
use crate::cli::commands::Command;
use crate::config::Config;
use crate::core::transaction::{SignedTransactionPayload, Transaction};
use crate::crypto::signature::SecretKeyBytes;
use crate::crypto::wallet::Wallet;
use crate::error::AppError;
use crate::network::p2p::P2pNode;
use crate::rpc::server;
use crate::rpc::types::RpcState;

const GENESIS_TIMESTAMP_UNIX: u64 = 1_700_000_000;
const DEFAULT_RPC_REQUEST_ID: u64 = 1;
const FAUCET_SECRET_KEY: [u8; 32] = [0x11; 32];
const FAUCET_GENESIS_BALANCE: u64 = 1_000_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalletFile {
    secret_key_hex: String,
    public_key_hex: String,
    address: String,
    faucet: bool,
}

#[derive(Debug, Deserialize)]
struct RpcResponseEnvelope {
    jsonrpc: String,
    result: Option<Value>,
    error: Option<RpcErrorEnvelope>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorEnvelope {
    code: i32,
    message: String,
}

pub async fn run(command: Option<Command>, config: Config) -> Result<(), AppError> {
    match command.unwrap_or(Command::StartNode) {
        Command::StartNode => run_node(config).await,
        Command::Mine {
            rpc_url,
            timestamp_unix,
            max_nonce,
        } => run_mine(&config, rpc_url.as_deref(), timestamp_unix, max_nonce).await,
        Command::Send {
            wallet,
            rpc_url,
            to,
            amount,
            fee,
            nonce,
        } => run_send(&config, rpc_url.as_deref(), &wallet, to, amount, fee, nonce).await,
        Command::GenerateWallet { out, faucet } => run_generate_wallet(&out, faucet),
    }
}

async fn run_node(config: Config) -> Result<(), AppError> {
    let chain_config = ChainConfig {
        difficulty_bits: config.mining.difficulty_bits,
        max_transactions_per_block: config.mining.max_transactions_per_block,
        genesis_timestamp_unix: GENESIS_TIMESTAMP_UNIX,
    };

    let faucet_wallet = faucet_wallet();
    let genesis_accounts = vec![GenesisAccount::from_public_key(
        &faucet_wallet.public_key_bytes(),
        FAUCET_GENESIS_BALANCE,
    )];

    let blockchain =
        Blockchain::open_or_init(&config.storage.path, chain_config, genesis_accounts)?;
    let state: RpcState = Arc::new(Mutex::new(blockchain));

    let p2p_node = P2pNode::start(
        &config.network.listen_addr,
        config.network.peers.clone(),
        Arc::clone(&state),
    )
    .await?;

    let (rpc_shutdown_tx, rpc_shutdown_rx) = oneshot::channel::<()>();
    let rpc_addr = server::serve(&config.rpc.listen_addr, Arc::clone(&state), async move {
        let _ = rpc_shutdown_rx.await;
    })
    .await?;

    tracing::info!(
        node_id = %config.node.id,
        chain_id = %config.node.chain_id,
        p2p_addr = %p2p_node.listen_addr(),
        rpc_addr = %rpc_addr,
        faucet_address = %faucet_wallet.address(),
        "node started"
    );

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutdown signal received");

    let _ = rpc_shutdown_tx.send(());
    p2p_node.shutdown().await;
    Ok(())
}

async fn run_mine(
    config: &Config,
    rpc_url_override: Option<&str>,
    timestamp_unix: Option<u64>,
    max_nonce: u64,
) -> Result<(), AppError> {
    let rpc_url = resolve_rpc_url(config, rpc_url_override)?;
    let params = json!({
        "timestamp_unix": timestamp_unix,
        "max_nonce": max_nonce,
    });

    let result = rpc_request(&rpc_url, "mine_block", Some(params)).await?;
    let block_hash = expect_string_field(&result, "block_hash")?;
    let height = expect_u64_field(&result, "height")?;

    println!("mined block");
    println!("block_hash={block_hash}");
    println!("height={height}");
    Ok(())
}

async fn run_send(
    config: &Config,
    rpc_url_override: Option<&str>,
    wallet_path: &Path,
    to: String,
    amount: u64,
    fee: u64,
    nonce: u64,
) -> Result<(), AppError> {
    let wallet = load_wallet(wallet_path)?;
    let rpc_url = resolve_rpc_url(config, rpc_url_override)?;

    let payload = SignedTransactionPayload {
        from: wallet.public_key_hex(),
        to,
        amount,
        fee,
        nonce,
    };
    let signature = wallet.sign_payload(&payload);
    let tx = Transaction {
        from: payload.from,
        to: payload.to,
        amount: payload.amount,
        fee: payload.fee,
        nonce: payload.nonce,
        signature: signature.0.to_vec(),
    };

    let result = rpc_request(&rpc_url, "send_transaction", Some(json!({ "tx": tx }))).await?;
    let tx_hash = expect_string_field(&result, "tx_hash")?;

    println!("submitted transaction");
    println!("tx_hash={tx_hash}");
    Ok(())
}

pub fn run_generate_wallet(path: &Path, faucet: bool) -> Result<(), AppError> {
    let wallet = if faucet {
        faucet_wallet()
    } else {
        Wallet::generate()
    };

    persist_wallet(path, &wallet, faucet)?;
    println!("wallet generated");
    println!("address={}", wallet.address());
    println!("public_key={}", wallet.public_key_hex());
    println!("path={}", path.display());
    println!("faucet={faucet}");
    Ok(())
}

fn faucet_wallet() -> Wallet {
    Wallet::from_secret_key(SecretKeyBytes(FAUCET_SECRET_KEY))
}

fn persist_wallet(path: &Path, wallet: &Wallet, faucet: bool) -> Result<(), AppError> {
    let file = WalletFile {
        secret_key_hex: wallet.secret_key_hex(),
        public_key_hex: wallet.public_key_hex(),
        address: wallet.address(),
        faucet,
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let encoded = serde_json::to_vec_pretty(&file)?;
    let tmp_path = temp_wallet_path(path);
    fs::write(&tmp_path, encoded)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(tmp_path, path)?;
    Ok(())
}

fn load_wallet(path: &Path) -> Result<Wallet, AppError> {
    let raw = fs::read_to_string(path).map_err(|error| AppError::WalletFile {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    let parsed: WalletFile = serde_json::from_str(&raw).map_err(|error| AppError::WalletFile {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    let wallet = Wallet::from_secret_key_hex(&parsed.secret_key_hex)?;

    if wallet.public_key_hex() != parsed.public_key_hex {
        return Err(AppError::WalletFile {
            path: path.to_path_buf(),
            reason: "public key does not match secret key".to_string(),
        });
    }

    if wallet.address() != parsed.address {
        return Err(AppError::WalletFile {
            path: path.to_path_buf(),
            reason: "address does not match secret key".to_string(),
        });
    }

    Ok(wallet)
}

fn temp_wallet_path(path: &Path) -> PathBuf {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => path.with_extension(format!("{ext}.tmp")),
        None => path.with_extension("tmp"),
    }
}

fn resolve_rpc_url(config: &Config, rpc_url_override: Option<&str>) -> Result<String, AppError> {
    let candidate = rpc_url_override.unwrap_or(&config.rpc.listen_addr);
    let normalized = if candidate.starts_with("http://") || candidate.starts_with("https://") {
        candidate.to_string()
    } else {
        format!("http://{candidate}")
    };

    let url = reqwest::Url::parse(&normalized)
        .map_err(|error| AppError::InvalidRpcUrl(error.to_string()))?;
    Ok(url.to_string())
}

async fn rpc_request(
    rpc_url: &str,
    method: &str,
    params: Option<Value>,
) -> Result<Value, AppError> {
    let client = reqwest::Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "id": DEFAULT_RPC_REQUEST_ID,
        "method": method,
        "params": params,
    });

    let response = client.post(rpc_url).json(&body).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::RpcHttpStatus(status.as_u16()));
    }

    let envelope: RpcResponseEnvelope = response.json().await?;
    if envelope.jsonrpc != "2.0" {
        return Err(AppError::InvalidRpcResponse(
            "jsonrpc version mismatch".to_string(),
        ));
    }

    if let Some(error) = envelope.error {
        return Err(AppError::Rpc {
            code: error.code,
            message: error.message,
        });
    }

    envelope
        .result
        .ok_or_else(|| AppError::InvalidRpcResponse("missing result field".to_string()))
}

fn expect_string_field(value: &Value, field: &'static str) -> Result<String, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::InvalidRpcResponse(format!("missing string field: {field}")))
}

fn expect_u64_field(value: &Value, field: &'static str) -> Result<u64, AppError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::InvalidRpcResponse(format!("missing u64 field: {field}")))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use crate::cli::runtime::{faucet_wallet, load_wallet, persist_wallet, resolve_rpc_url};
    use crate::config::{
        Config, LoggingConfig, MiningConfig, NetworkConfig, NodeConfig, RpcConfig, StorageConfig,
    };
    use crate::error::AppError;

    fn sample_config() -> Config {
        Config {
            node: NodeConfig {
                id: "node".to_string(),
                chain_id: "chain".to_string(),
            },
            network: NetworkConfig {
                listen_addr: "127.0.0.1:6000".to_string(),
                peers: Vec::new(),
            },
            rpc: RpcConfig {
                listen_addr: "127.0.0.1:7000".to_string(),
            },
            storage: StorageConfig {
                path: PathBuf::from("./data"),
            },
            mining: MiningConfig {
                difficulty_bits: 0,
                max_transactions_per_block: 1000,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
        }
    }

    #[test]
    fn wallet_file_roundtrip() -> Result<(), AppError> {
        let dir = tempdir()?;
        let path = dir.path().join("wallet.json");
        let wallet = faucet_wallet();

        persist_wallet(&path, &wallet, true)?;
        let loaded = load_wallet(&path)?;

        assert_eq!(wallet.secret_key_hex(), loaded.secret_key_hex());
        assert_eq!(wallet.public_key_hex(), loaded.public_key_hex());
        assert_eq!(wallet.address(), loaded.address());
        Ok(())
    }

    #[test]
    fn load_wallet_rejects_tampered_metadata() -> Result<(), AppError> {
        let dir = tempdir()?;
        let path = dir.path().join("wallet.json");
        let wallet = faucet_wallet();
        persist_wallet(&path, &wallet, true)?;

        let raw = fs::read_to_string(&path)?;
        let tampered = raw.replace("\"address\": \"", "\"address\": \"rc1tampered");
        fs::write(&path, tampered)?;

        let loaded = load_wallet(&path);
        assert!(matches!(loaded, Err(AppError::WalletFile { .. })));
        Ok(())
    }

    #[test]
    fn rpc_url_resolution_supports_override_and_config() -> Result<(), AppError> {
        let config = sample_config();
        let from_config = resolve_rpc_url(&config, None)?;
        assert_eq!(from_config, "http://127.0.0.1:7000/");

        let override_url = resolve_rpc_url(&config, Some("http://127.0.0.1:9999"))?;
        assert_eq!(override_url, "http://127.0.0.1:9999/");
        Ok(())
    }
}
