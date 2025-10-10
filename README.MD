# Proof Job Marketplace

A decentralized proof marketplace where users submit computation jobs, workers generate ZK proofs using RISC Zero zkVM, and proofs are verified on-chain.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- [Foundry](https://getfoundry.sh/) (for Anvil and contract deployment)
- [RISC Zero](https://dev.risczero.com/api/zkvm/install) (`rzup install`)

### Setup

```bash
# Build crates, start Anvil, deploy contracts (saves REGISTRY_ADDRESS to .env)
make setup
```

### Run the Demo

```bash
# Option 1: Full automated demo
make demo

# Option 2: Manual (separate terminals)

# Terminal 1: Anvil (if not running)
make anvil

# Terminal 2: Coordinator
make coordinator-local

# Terminal 3: Worker 1
make worker-local

# Terminal 4: Worker 2
make worker-2-local

# Terminal 5: Submit job
make submit-job N=10

# Check status
make check-status JOB_ID=0x...
```

## How It Works

### Job Flow

```
1. User POST /jobs to Coordinator
2. Coordinator calls JobRegistry.submitJob() on-chain (locks ETH)
3. Coordinator broadcasts JobAvailable via gossip (and re-broadcasts periodically for late joiners)
4. Workers receive job, one claims it (first-wins arbitration)
5. Winning worker generates ZK proof using RISC Zero
6. Worker calls JobRegistry.submitProof() on-chain
7. Contract verifies proof, pays worker
8. Indexer detects JobCompleted event, updates Coordinator
9. GET /jobs/:id/status shows Completed with tx_hash
```

### Components

| Component       | Description                                             |
| --------------- | ------------------------------------------------------- |
| **Coordinator** | HTTP API, job registry, gossip node, claim arbitration  |
| **Worker**      | Gossip node, RISC Zero prover, on-chain proof submitter |
| **Gossip**      | P2P message propagation (TCP, length-prefixed, bincode) |
| **Contracts**   | Job escrow, RISC Zero verification, payment             |
| **Indexer**     | Chain event listener, state synchronization             |

### Crate Structure

```
crates/
  core/           # Shared types: JobId, Job, GossipMessage, etc.
  gossip/         # P2P protocol implementation
  methods/        # RISC Zero guest program (fibonacci)
  prover/         # Async prove() wrapper
  worker/         # Worker binary
  coordinator/    # Coordinator binary
  contract-client/# On-chain interaction client
  indexer/        # Chain event indexer
  e2e/            # End-to-end integration tests
contracts/
  src/JobRegistry.sol
```

## API

### POST /jobs

Submit a new job.

```bash
curl -X POST http://localhost:8080/jobs \
  -H "Content-Type: application/json" \
  -d '{"payload": "0x0a00000000000000", "reward": 1000000000000000000}'
```

Response:

```json
{
  "job_id": "0x1234...",
  "tx_hash": "0xabcd..."
}
```

### GET /jobs/:id

Get job details.

```bash
curl http://localhost:8080/jobs/0x1234...
```

### GET /jobs/:id/status

Get job status.

```bash
curl http://localhost:8080/jobs/0x1234.../status
```

Response:

```json
{
  "state": "Completed",
  "tx_hash": "0xabcd..."
}
```

## Configuration

Environment variables:

| Variable             | Description                         | Default                         |
| -------------------- | ----------------------------------- | ------------------------------- |
| `REGISTRY_ADDRESS`   | Deployed JobRegistry address        | (required)                      |
| `RPC_URL`            | Ethereum RPC endpoint               | `http://127.0.0.1:8545`         |
| `HTTP_PORT`          | Coordinator HTTP port               | `8080`                          |
| `GOSSIP_PORT`        | Gossip listen port                  | `9000` (coord), `9001` (worker) |
| `PRIVATE_KEY`        | Wallet private key                  | Anvil account #0/#1             |
| `RISC0_DEV_MODE`     | Use fake proofs                     | `1` for testing                 |
| `COORDINATOR_GOSSIP` | Coordinator gossip address (worker) | `127.0.0.1:9000`                |

## Testing

```bash
# Unit tests
RISC0_DEV_MODE=1 cargo test --workspace

# E2E test (requires Anvil running + contract deployed)
make setup
make test-e2e
```

## Architecture Notes

- **Gossip Protocol**: Epidemic broadcast with TTL=3, fanout=3, LRU deduplication
- **Claim Arbitration**: Coordinator-side first-wins (centralized but fast)
- **Proof Verification**: MockVerifier on Anvil; real RISC Zero verifier for production
- **State Sync**: Dual-path updates via gossip (fast) and indexer (reliable)
