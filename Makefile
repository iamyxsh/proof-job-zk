ANVIL_PRIVATE_KEY := 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
IMAGE_ID := 0x0000000000000000000000000000000000000000000000000000000000012345
JOB_REGISTRY := 0x1a817E66a30c5cFD1c56A71F3E8edE08eA705070

.PHONY: gossip build check test test-prover test-prover-real test-core test-coordinator test-all test-contracts deploy-local deploy-deterministic predict-addresses anvil coordinator-local clean

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

test-coordinator:
	cargo test -p coordinator -- --nocapture

test-all:
	RISC0_DEV_MODE=1 cargo test --workspace -- --nocapture

test-contracts:
	cd contracts && forge test -vvv

anvil:
	anvil

deploy-local:
	cd contracts && PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) IMAGE_ID=$(IMAGE_ID) forge script script/Deploy.s.sol --rpc-url http://127.0.0.1:8545 --broadcast

deploy-deterministic:
	cd contracts && PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) IMAGE_ID=$(IMAGE_ID) forge script script/DeployDeterministic.s.sol --rpc-url http://127.0.0.1:8545 --broadcast

predict-addresses:
	cd contracts && PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) IMAGE_ID=$(IMAGE_ID) forge script script/DeployDeterministic.s.sol --sig "predict()"

coordinator-local:
	RPC_URL=http://127.0.0.1:8545 CONTRACT_ADDRESS=$(JOB_REGISTRY) PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) cargo run -p coordinator

clean:
	cargo clean
	cd contracts && forge clean
