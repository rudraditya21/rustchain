use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command as StdCommand, Stdio};
use std::time::Duration;

use predicates::prelude::*;
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::time::{sleep, Instant};

fn cargo_cmd() -> Result<assert_cmd::Command, String> {
    Ok(assert_cmd::Command::new(env!("CARGO_BIN_EXE_rustchain")))
}

fn run_cli(current_dir: &Path, args: &[String]) -> Result<std::process::Output, String> {
    StdCommand::new(env!("CARGO_BIN_EXE_rustchain"))
        .current_dir(current_dir)
        .args(args)
        .output()
        .map_err(|error| error.to_string())
}

fn take_free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| error.to_string())
}

fn write_config(
    path: &Path,
    storage_path: &Path,
    p2p_port: u16,
    rpc_port: u16,
) -> Result<(), String> {
    let storage = storage_path.to_string_lossy();
    let toml = format!(
        r#"[node]
id = "cli-test-node"
chain_id = "rustchain-local"

[network]
listen_addr = "127.0.0.1:{p2p_port}"
peers = []

[rpc]
listen_addr = "127.0.0.1:{rpc_port}"

[storage]
path = "{storage}"

[mining]
difficulty_bits = 0
max_transactions_per_block = 1000

[logging]
level = "info"
"#
    );

    fs::write(path, toml).map_err(|error| error.to_string())
}

async fn rpc_call(rpc_url: &str, method: &str, params: Option<Value>) -> Result<Value, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let response = reqwest::Client::new()
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("unexpected HTTP status: {}", response.status()));
    }

    let envelope: Value = response.json().await.map_err(|error| error.to_string())?;
    if let Some(error) = envelope.get("error") {
        return Err(format!("rpc error: {error}"));
    }

    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| "missing result field".to_string())
}

async fn wait_for_rpc_ready(rpc_url: &str, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if rpc_call(rpc_url, "get_chain", None).await.is_ok() {
            return Ok(true);
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(false)
}

fn terminate_child(child: &mut Child) -> Result<(), String> {
    if let Err(error) = child.kill() {
        if error.kind() != std::io::ErrorKind::InvalidInput {
            return Err(error.to_string());
        }
    }
    let _ = child.wait().map_err(|error| error.to_string())?;
    Ok(())
}

#[test]
fn cli_help_contains_required_commands() -> Result<(), String> {
    let mut command = cargo_cmd()?;
    command.arg("--help");
    command.assert().success().stdout(
        predicate::str::contains("start-node")
            .and(predicate::str::contains("mine"))
            .and(predicate::str::contains("send"))
            .and(predicate::str::contains("generate-wallet")),
    );
    Ok(())
}

#[test]
fn cli_send_requires_nonce() -> Result<(), String> {
    let mut command = cargo_cmd()?;
    command
        .arg("send")
        .arg("--to")
        .arg("rc1recipient")
        .arg("--amount")
        .arg("1");
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("--nonce"));
    Ok(())
}

#[test]
fn generate_wallet_faucet_output_matches_golden() -> Result<(), String> {
    let dir = tempdir().map_err(|error| error.to_string())?;
    let mut command = cargo_cmd()?;
    command
        .current_dir(dir.path())
        .arg("generate-wallet")
        .arg("--faucet")
        .arg("--out")
        .arg("wallet.json");

    let assert = command.assert().success();
    let stdout =
        String::from_utf8(assert.get_output().stdout.clone()).map_err(|error| error.to_string())?;

    let expected = "\
wallet generated
address=rc110ba682c8ad13513971e8b56881aab8bd702bb80
public_key=d04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737
path=wallet.json
faucet=true
";
    assert_eq!(stdout, expected);

    let wallet_file = dir.path().join("wallet.json");
    if !wallet_file.exists() {
        return Err("wallet file was not created".to_string());
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn required_commands_work_against_running_node() -> Result<(), String> {
    let dir = tempdir().map_err(|error| error.to_string())?;
    let storage_path = dir.path().join("data");
    let config_path = dir.path().join("node.toml");
    let faucet_path = dir.path().join("faucet.json");
    let recipient_path = dir.path().join("recipient.json");

    let p2p_port = take_free_port()?;
    let rpc_port = take_free_port()?;
    write_config(&config_path, &storage_path, p2p_port, rpc_port)?;

    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_rustchain"))
        .current_dir(dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("start-node")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let rpc_url = format!("http://127.0.0.1:{rpc_port}");

    let started = wait_for_rpc_ready(&rpc_url, Duration::from_secs(8)).await?;
    if !started {
        let _ = terminate_child(&mut child);
        return Err("node did not become ready".to_string());
    }

    let generate_faucet_args = vec![
        "--config".to_string(),
        config_path.display().to_string(),
        "generate-wallet".to_string(),
        "--faucet".to_string(),
        "--out".to_string(),
        faucet_path.display().to_string(),
    ];
    let faucet_output = run_cli(dir.path(), &generate_faucet_args)?;
    if !faucet_output.status.success() {
        let _ = terminate_child(&mut child);
        return Err(format!(
            "generate faucet wallet failed: {}",
            String::from_utf8_lossy(&faucet_output.stderr)
        ));
    }

    let generate_recipient_args = vec![
        "--config".to_string(),
        config_path.display().to_string(),
        "generate-wallet".to_string(),
        "--out".to_string(),
        recipient_path.display().to_string(),
    ];
    let recipient_output = run_cli(dir.path(), &generate_recipient_args)?;
    if !recipient_output.status.success() {
        let _ = terminate_child(&mut child);
        return Err(format!(
            "generate recipient wallet failed: {}",
            String::from_utf8_lossy(&recipient_output.stderr)
        ));
    }

    let recipient_wallet_raw =
        fs::read_to_string(&recipient_path).map_err(|error| error.to_string())?;
    let recipient_wallet_json: Value =
        serde_json::from_str(&recipient_wallet_raw).map_err(|error| error.to_string())?;
    let recipient_address = recipient_wallet_json
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| "recipient wallet missing address".to_string())?
        .to_string();

    let send_args = vec![
        "--config".to_string(),
        config_path.display().to_string(),
        "send".to_string(),
        "--wallet".to_string(),
        faucet_path.display().to_string(),
        "--to".to_string(),
        recipient_address.clone(),
        "--amount".to_string(),
        "25".to_string(),
        "--fee".to_string(),
        "1".to_string(),
        "--nonce".to_string(),
        "1".to_string(),
    ];
    let send_output = run_cli(dir.path(), &send_args)?;
    if !send_output.status.success() {
        let _ = terminate_child(&mut child);
        return Err(format!(
            "send command failed: {}",
            String::from_utf8_lossy(&send_output.stderr)
        ));
    }

    let mine_args = vec![
        "--config".to_string(),
        config_path.display().to_string(),
        "mine".to_string(),
        "--timestamp-unix".to_string(),
        "1700040000".to_string(),
        "--max-nonce".to_string(),
        "0".to_string(),
    ];
    let mine_output = run_cli(dir.path(), &mine_args)?;
    if !mine_output.status.success() {
        let _ = terminate_child(&mut child);
        return Err(format!(
            "mine command failed: {}",
            String::from_utf8_lossy(&mine_output.stderr)
        ));
    }

    let mined_stdout = String::from_utf8(mine_output.stdout).map_err(|error| error.to_string())?;
    if !mined_stdout.contains("height=1") {
        let _ = terminate_child(&mut child);
        return Err(format!("unexpected mine output: {mined_stdout}"));
    }

    let balance_result = rpc_call(
        &rpc_url,
        "get_balance",
        Some(json!({ "address": recipient_address })),
    )
    .await?;
    let balance = balance_result
        .get("balance")
        .and_then(Value::as_u64)
        .ok_or_else(|| "missing balance in RPC response".to_string())?;

    let shutdown_result = terminate_child(&mut child);
    if let Err(error) = shutdown_result {
        return Err(format!("failed to stop node process: {error}"));
    }

    if balance != 25 {
        return Err(format!("unexpected recipient balance: {balance}"));
    }
    Ok(())
}
