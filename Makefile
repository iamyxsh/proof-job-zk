.PHONY: gossip build check test test-prover test-prover-real test-core test-all test-contracts deploy-local anvil clean

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

test-contracts:
	cd contracts && forge test -vvv

anvil:
	anvil

deploy-local:
	cd contracts && PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 IMAGE_ID=0x0000000000000000000000000000000000000000000000000000000000012345 forge script script/Deploy.s.sol --rpc-url http://127.0.0.1:8545 --broadcast

clean:
	cargo clean
	cd contracts && forge clean
