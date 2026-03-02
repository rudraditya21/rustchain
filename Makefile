.PHONY: fmt clippy test bench coverage ci

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --locked -- -D warnings

test:
	cargo test --locked

bench:
	cargo bench --bench mining

coverage:
	cargo llvm-cov --workspace --all-features --tests --fail-under-lines 80 --summary-only

ci: fmt clippy test coverage
