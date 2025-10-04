ANVIL_PRIVATE_KEY := 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
ANVIL_KEY_1 := 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
ANVIL_KEY_2 := 0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a
IMAGE_ID := 0x0000000000000000000000000000000000000000000000000000000000012345

.PHONY: gossip build check test test-prover test-prover-real test-core test-coordinator test-all test-contracts deploy-local deploy-deterministic predict-addresses anvil coordinator-local worker-local worker-2-local setup demo submit-job check-status test-e2e clean

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
	RPC_URL=http://127.0.0.1:8545 CONTRACT_ADDRESS=$$(cat .env 2>/dev/null | grep REGISTRY_ADDRESS | cut -d= -f2 || echo $(JOB_REGISTRY)) PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) RUST_LOG=coordinator=debug,indexer=debug,gossip=info cargo run --release -p coordinator

worker-local:
	RISC0_DEV_MODE=1 REGISTRY_ADDRESS=$$(cat .env 2>/dev/null | grep REGISTRY_ADDRESS | cut -d= -f2 || echo $(JOB_REGISTRY)) RPC_URL=http://127.0.0.1:8545 GOSSIP_PORT=9001 COORDINATOR_GOSSIP=127.0.0.1:9000 PRIVATE_KEY=$(ANVIL_KEY_1) RUST_LOG=worker=debug,prover=info,gossip=info cargo run --release -p worker

worker-2-local:
	RISC0_DEV_MODE=1 REGISTRY_ADDRESS=$$(cat .env 2>/dev/null | grep REGISTRY_ADDRESS | cut -d= -f2 || echo $(JOB_REGISTRY)) RPC_URL=http://127.0.0.1:8545 GOSSIP_PORT=9002 COORDINATOR_GOSSIP=127.0.0.1:9000 PRIVATE_KEY=$(ANVIL_KEY_2) RUST_LOG=worker=debug,prover=info,gossip=info cargo run --release -p worker

# Build crates, start Anvil if needed, deploy contracts, save REGISTRY_ADDRESS to .env
setup:
	@echo "=== Proof Job Marketplace Setup ==="
	@command -v anvil >/dev/null 2>&1 || { echo "anvil not found. Install Foundry: https://getfoundry.sh"; exit 1; }
	@command -v forge >/dev/null 2>&1 || { echo "forge not found. Install Foundry: https://getfoundry.sh"; exit 1; }
	@echo "[1/3] Building Rust crates..."
	@RISC0_DEV_MODE=1 cargo build --release
	@if ! curl -s http://127.0.0.1:8545 > /dev/null 2>&1; then \
		echo "[2/3] Starting Anvil..."; \
		anvil & sleep 2; \
	else \
		echo "[2/3] Anvil already running"; \
	fi
	@echo "[3/3] Deploying contracts..."
	@DEPLOY_OUTPUT=$$(cd contracts && PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) IMAGE_ID=$(IMAGE_ID) forge script script/Deploy.s.sol --rpc-url http://127.0.0.1:8545 --broadcast 2>&1); \
	REGISTRY_ADDRESS=$$(echo "$$DEPLOY_OUTPUT" | grep -o "JobRegistry deployed at: 0x[a-fA-F0-9]*" | grep -o "0x[a-fA-F0-9]*"); \
	if [ -z "$$REGISTRY_ADDRESS" ]; then \
		echo "Failed to extract registry address"; echo "$$DEPLOY_OUTPUT"; exit 1; \
	fi; \
	echo "REGISTRY_ADDRESS=$$REGISTRY_ADDRESS" > .env; \
	echo "=== Setup Complete ==="; \
	echo "Registry: $$REGISTRY_ADDRESS (saved to .env)"

# Submit a job: make submit-job N=10 REWARD=1000000000000000000
N ?= 10
REWARD ?= 1000000000000000000
submit-job:
	@PAYLOAD=$$(printf '%016x' $(N) | fold -w2 | tac | tr -d '\n'); \
	echo "Submitting fib($(N)) with reward $(REWARD) wei"; \
	curl -s -X POST http://127.0.0.1:8080/jobs \
		-H "Content-Type: application/json" \
		-d "{\"payload\": \"0x$$PAYLOAD\", \"reward\": $(REWARD)}" | python3 -m json.tool

# Check job status: make check-status JOB_ID=0x...
check-status:
	@curl -s "http://127.0.0.1:8080/jobs/$(JOB_ID)/status" | python3 -m json.tool

# Full automated demo: setup -> coordinator -> 2 workers -> submit job -> poll to completion
demo:
	@PIDS=""; \
	cleanup() { echo ""; echo "[Cleanup] Stopping..."; kill $$PIDS 2>/dev/null; wait 2>/dev/null; }; \
	trap cleanup EXIT; \
	\
	echo "=== Proof Job Marketplace Demo ==="; \
	\
	if [ ! -f .env ]; then echo "Run 'make setup' first."; exit 1; fi; \
	export $$(cat .env | xargs); \
	\
	echo "[1] Starting Coordinator..."; \
	RPC_URL=http://127.0.0.1:8545 CONTRACT_ADDRESS=$$REGISTRY_ADDRESS PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) \
		RUST_LOG=coordinator=info,indexer=info,gossip=warn \
		cargo run --release -p coordinator & \
	PIDS="$$PIDS $$!"; sleep 3; \
	if ! curl -s http://127.0.0.1:8080/health > /dev/null 2>&1; then echo "Coordinator failed"; exit 1; fi; \
	echo "    Coordinator running"; \
	\
	echo "[2] Starting Worker 1..."; \
	RISC0_DEV_MODE=1 REGISTRY_ADDRESS=$$REGISTRY_ADDRESS RPC_URL=http://127.0.0.1:8545 \
		GOSSIP_PORT=9001 COORDINATOR_GOSSIP=127.0.0.1:9000 PRIVATE_KEY=$(ANVIL_KEY_1) \
		RUST_LOG=worker=info,prover=info,gossip=warn \
		cargo run --release -p worker & \
	PIDS="$$PIDS $$!"; sleep 2; \
	\
	echo "[3] Starting Worker 2..."; \
	RISC0_DEV_MODE=1 REGISTRY_ADDRESS=$$REGISTRY_ADDRESS RPC_URL=http://127.0.0.1:8545 \
		GOSSIP_PORT=9002 COORDINATOR_GOSSIP=127.0.0.1:9000 PRIVATE_KEY=$(ANVIL_KEY_2) \
		RUST_LOG=worker=info,prover=info,gossip=warn \
		cargo run --release -p worker & \
	PIDS="$$PIDS $$!"; sleep 2; \
	\
	echo "[4] Submitting job: fib(10) with 1 ETH reward..."; \
	RESPONSE=$$(curl -s -X POST http://127.0.0.1:8080/jobs \
		-H "Content-Type: application/json" \
		-d '{"payload": "0x0a00000000000000", "reward": 1000000000000000000}'); \
	echo "    $$RESPONSE"; \
	JOB_ID=$$(echo "$$RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['job_id'])" 2>/dev/null); \
	if [ -z "$$JOB_ID" ]; then echo "Failed to parse job_id"; exit 1; fi; \
	echo "    Job ID: $$JOB_ID"; \
	\
	echo "[5] Polling for completion..."; \
	for i in $$(seq 1 30); do \
		STATUS=$$(curl -s "http://127.0.0.1:8080/jobs/$$JOB_ID/status"); \
		STATE=$$(echo "$$STATUS" | python3 -c "import sys,json; print(json.load(sys.stdin)['state'])" 2>/dev/null || echo "unknown"); \
		echo "    [$$i] $$STATE"; \
		if [ "$$STATE" = "Completed" ]; then \
			echo ""; echo "=== JOB COMPLETED ==="; \
			echo "$$STATUS" | python3 -m json.tool; \
			break; \
		fi; \
		if [ "$$i" -eq 30 ]; then echo "TIMEOUT"; exit 1; fi; \
		sleep 2; \
	done; \
	echo ""; echo "=== Demo Complete ==="

test-e2e:
	@echo "Requires Anvil running and contracts deployed. Run 'make setup' first."
	@test -f .env && export $$(cat .env | xargs) && \
		RUN_E2E=1 RISC0_DEV_MODE=1 cargo test -p e2e full_e2e_success_criteria -- --nocapture

clean:
	cargo clean
	cd contracts && forge clean
