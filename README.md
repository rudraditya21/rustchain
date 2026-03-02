# rustchain

Production-style minimal blockchain node in Rust with:
- deterministic core primitives (`SHA-256`, Merkle, PoW)
- signed transactions (`ed25519`)
- persistent chain/mempool storage (`sled`)
- Tokio TCP P2P propagation/sync
- Axum JSON-RPC
- Clap CLI for node operations

## Architecture

```text
src/
  blockchain/  # chain engine, mempool, validation, reorg
  core/        # tx/block data model, canonical encoding, hash, merkle
  crypto/      # key management, signing and verification
  network/     # async TCP P2P protocol and sync
  storage/     # sled schema + persistence
  rpc/         # JSON-RPC server and handlers
  cli/         # command parsing + runtime command execution
  config.rs    # TOML config loading
  logging.rs   # tracing setup
  lib.rs       # library exports for tests/benches
  main.rs      # binary entrypoint
```

## Quick Start (Local)

1. Build:
```bash
cargo build --locked
```

2. Start node:
```bash
cargo run --locked -- start-node
```

3. Generate wallets:
```bash
cargo run --locked -- generate-wallet --faucet --out faucet.json
cargo run --locked -- generate-wallet --out receiver.json
```

4. Send tx (example):
```bash
cargo run --locked -- send \
  --wallet faucet.json \
  --to rc1...receiver_address... \
  --amount 25 \
  --fee 1 \
  --nonce 1
```

5. Mine block:
```bash
cargo run --locked -- mine --timestamp-unix 1700040000 --max-nonce 0
```

## CLI Commands

```bash
rustchain start-node
rustchain mine --rpc-url http://127.0.0.1:7000 --timestamp-unix 1700040000 --max-nonce 1000000
rustchain send --wallet wallet.json --to <address> --amount <u64> --fee <u64> --nonce <u64>
rustchain generate-wallet --out wallet.json [--faucet]
```

Global config override:
```bash
rustchain --config ./config/default.toml <command>
```

## JSON-RPC API

Server defaults to `http://127.0.0.1:7000`.

`get_chain`:
```bash
curl -s http://127.0.0.1:7000 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"get_chain"}'
```

`get_balance`:
```bash
curl -s http://127.0.0.1:7000 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"get_balance","params":{"address":"rc1..."}}'
```

`send_transaction`:
```bash
curl -s http://127.0.0.1:7000 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"send_transaction","params":{"tx":{"from":"<pubkey_hex>","to":"rc1...","amount":25,"fee":1,"nonce":1,"signature":[1,2,3]}}}'
```

`mine_block`:
```bash
curl -s http://127.0.0.1:7000 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":4,"method":"mine_block","params":{"timestamp_unix":1700040000,"max_nonce":1000000}}'
```

## Quality Gates

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Coverage (target: >= 80% lines):
```bash
cargo llvm-cov --workspace --all-features --tests --fail-under-lines 80 --summary-only
```

## Mining Benchmark

Run Criterion benchmark:
```bash
cargo bench --bench mining
```

## Docker

Build image:
```bash
docker build -t rustchain:latest .
```

Run node:
```bash
docker run --rm -p 6000:6000 -p 7000:7000 -v "$(pwd)/data:/app/data" rustchain:latest
```

Override config:
```bash
docker run --rm -p 6000:6000 -p 7000:7000 \
  -v "$(pwd)/config/default.toml:/app/config/default.toml:ro" \
  -v "$(pwd)/data:/app/data" \
  rustchain:latest --config /app/config/default.toml start-node
```
