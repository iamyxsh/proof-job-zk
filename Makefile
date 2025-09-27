.PHONY: gossip build check test test-prover test-prover-real test-core test-all clean

gossip:
	cargo run -p gossip

build:
	cargo build --workspace

check:
	cargo check --workspace

test:
	cargo test --workspace

test-prover:
	RISC0_DEV_MODE=1 cargo test -p prover -- --nocapture

test-prover-real:
	cargo test -p prover -- --nocapture

test-core:
	cargo test -p core

test-all:
	RISC0_DEV_MODE=1 cargo test --workspace -- --nocapture

clean:
	cargo clean
