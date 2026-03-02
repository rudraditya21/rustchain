use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tempfile::TempDir;

use rustchain::blockchain::chain::{Blockchain, ChainConfig};
use rustchain::blockchain::state::GenesisAccount;
use rustchain::core::block::Block;
use rustchain::core::transaction::{SignedTransactionPayload, Transaction};
use rustchain::crypto::signature::SecretKeyBytes;
use rustchain::crypto::wallet::Wallet;

struct MiningFixture {
    _dir: TempDir,
    chain: Blockchain,
    candidate: Block,
}

fn build_fixture(difficulty_bits: u32) -> Result<MiningFixture, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let sender = Wallet::from_secret_key(SecretKeyBytes([0x21; 32]));
    let recipient = Wallet::from_secret_key(SecretKeyBytes([0x22; 32]));
    let genesis = vec![GenesisAccount::from_public_key(
        &sender.public_key_bytes(),
        10_000_000,
    )];

    let mut chain = Blockchain::open_or_init(
        dir.path(),
        ChainConfig {
            difficulty_bits,
            max_transactions_per_block: 1_000,
            genesis_timestamp_unix: 1_700_000_000,
        },
        genesis,
    )
    .map_err(|error| error.to_string())?;

    let payload = SignedTransactionPayload {
        from: sender.public_key_hex(),
        to: recipient.address(),
        amount: 10_000,
        fee: 1,
        nonce: 1,
    };
    let signature = sender.sign_payload(&payload);
    let tx = Transaction {
        from: payload.from,
        to: payload.to,
        amount: payload.amount,
        fee: payload.fee,
        nonce: payload.nonce,
        signature: signature.0.to_vec(),
    };

    chain
        .admit_transaction(tx)
        .map_err(|error| error.to_string())?;
    let candidate = chain.build_candidate_block(1_700_000_010);

    Ok(MiningFixture {
        _dir: dir,
        chain,
        candidate,
    })
}

fn mining_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("pow_mining");
    for difficulty_bits in [12u32, 16u32] {
        let fixture = match build_fixture(difficulty_bits) {
            Ok(fixture) => fixture,
            Err(error) => panic!("failed to build mining fixture: {error}"),
        };

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("difficulty_{difficulty_bits}")),
            &difficulty_bits,
            |b, _| {
                b.iter(|| {
                    let mined = fixture
                        .chain
                        .mine_candidate_block(fixture.candidate.clone(), 2_000_000);
                    match mined {
                        Ok(block) => black_box(block.hash()),
                        Err(error) => panic!("mining benchmark failed: {error}"),
                    }
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, mining_benchmark);
criterion_main!(benches);
