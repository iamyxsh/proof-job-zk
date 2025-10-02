//! On-chain proof submission integration tests.
//!
//! Requires Anvil + deployed contracts + REGISTRY_ADDRESS env var + RISC0_DEV_MODE=1.
//!
//! Run:
//!   anvil &
//!   cd contracts && forge script script/Deploy.s.sol \
//!     --rpc-url http://127.0.0.1:8545 --broadcast \
//!     --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
//!   export REGISTRY_ADDRESS=0x...
//!   RISC0_DEV_MODE=1 cargo test -p worker --test onchain_tests -- --nocapture --ignored

use std::time::Duration;

use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::setup_test_coordinator;
use contract_client::ContractClient;
use gossip::gossip::{GossipConfig, GossipNode};
use proof_core::enums::{GossipMessage, JobStatus};
use proof_core::ids::JobId;
use proof_core::job::Job;
use tokio::time::{sleep, timeout};
use worker::{Worker, WorkerConfig, WorkerStatus};

const ANVIL_ACCOUNT_0_KEY: &str =
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_ACCOUNT_1_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const ANVIL_ACCOUNT_1_ADDR: &str = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";

const RPC_URL: &str = "http://127.0.0.1:8545";

fn registry_address() -> Address {
    std::env::var("REGISTRY_ADDRESS")
        .expect("REGISTRY_ADDRESS env var required")
        .parse()
        .expect("invalid REGISTRY_ADDRESS")
}

fn make_fib_job(job_id: JobId, n: u64, reward: u128) -> Job {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Job {
        id: job_id,
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: n.to_le_bytes().to_vec(),
        reward,
        created_at: now,
        deadline: now + 300,
    }
}

async fn submit_job_onchain(
    registry_address: Address,
    job_id: [u8; 32],
    payload: &[u8],
    deadline: u64,
    reward_wei: u128,
) {
    let client = ContractClient::new(RPC_URL, registry_address, ANVIL_ACCOUNT_0_KEY).unwrap();
    client
        .submit_job(job_id, payload, deadline, reward_wei)
        .await
        .expect("failed to submit job on-chain");
}

async fn is_job_completed_onchain(registry_address: Address, job_id: [u8; 32]) -> bool {
    let client = ContractClient::new(RPC_URL, registry_address, ANVIL_ACCOUNT_0_KEY).unwrap();
    client
        .is_completed(job_id)
        .await
        .expect("failed to check job completion")
}

async fn get_balance(addr: Address) -> U256 {
    let provider = ProviderBuilder::new().connect_http(RPC_URL.parse().unwrap());
    provider.get_balance(addr).await.expect("failed to get balance")
}

#[tokio::test]
#[ignore]
async fn worker_submits_proof_onchain() {
    let _ = tracing_subscriber::fmt::try_init();

    let reg_addr = registry_address();
    let worker_addr: Address = ANVIL_ACCOUNT_1_ADDR.parse().unwrap();
    let reward: u128 = 1_000_000_000_000_000_000;

    let coord_state = setup_test_coordinator().await;
    let gossip_addr = coord_state.gossip.local_addr().unwrap();
    tokio::spawn({
        let g = coord_state.gossip.clone();
        async move { g.run().await }
    });
    tokio::spawn(run_gossip_handler(
        coord_state.clone(),
        coord_state.gossip.subscribe(),
    ));

    let worker_config = WorkerConfig {
        gossip_addr: "127.0.0.1:0".parse().unwrap(),
        coordinator_addr: gossip_addr,
        rpc_url: RPC_URL.to_string(),
        private_key: ANVIL_ACCOUNT_1_KEY.to_string(),
        registry_address: reg_addr,
    };
    let worker = Worker::new(worker_config).await.unwrap();
    tokio::spawn({
        let g = worker.gossip.clone();
        async move { g.run().await }
    });
    worker.start().await.unwrap();
    tokio::spawn({
        let w = worker.clone();
        async move { w.run().await }
    });
    sleep(Duration::from_millis(200)).await;

    let job_id = JobId([0xaa; 32]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let deadline = now + 300;
    let payload = 10u64.to_le_bytes().to_vec();

    submit_job_onchain(reg_addr, job_id.0, &payload, deadline, reward).await;

    let balance_before = get_balance(worker_addr).await;

    let job = make_fib_job(job_id, 10, reward);
    coord_state.jobs.insert(job_id, job.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job))
        .await
        .unwrap();

    let result = timeout(Duration::from_secs(30), async {
        loop {
            sleep(Duration::from_secs(1)).await;
            let status = worker.status.read().await;
            if matches!(&*status, WorkerStatus::Idle) {
                if is_job_completed_onchain(reg_addr, job_id.0).await {
                    return;
                }
            }
        }
    })
    .await;

    assert!(result.is_ok(), "timeout waiting for job completion");

    let status = worker.status.read().await;
    assert!(
        matches!(&*status, WorkerStatus::Idle),
        "expected Idle, got {:?}",
        &*status
    );
    drop(status);

    assert!(
        is_job_completed_onchain(reg_addr, job_id.0).await,
        "job should be completed on-chain"
    );

    let balance_after = get_balance(worker_addr).await;
    assert!(
        balance_after > balance_before,
        "worker balance should have increased. before={}, after={}",
        balance_before,
        balance_after
    );
}

#[tokio::test]
#[ignore]
async fn worker_broadcasts_job_completed() {
    let _ = tracing_subscriber::fmt::try_init();

    let reg_addr = registry_address();
    let reward: u128 = 500_000_000_000_000_000;

    let coord_state = setup_test_coordinator().await;
    let gossip_addr = coord_state.gossip.local_addr().unwrap();
    tokio::spawn({
        let g = coord_state.gossip.clone();
        async move { g.run().await }
    });
    tokio::spawn(run_gossip_handler(
        coord_state.clone(),
        coord_state.gossip.subscribe(),
    ));

    let spy_config = GossipConfig::new("127.0.0.1:0".parse().unwrap());
    let spy = GossipNode::new(spy_config).await.unwrap();
    let mut spy_rx = spy.subscribe();
    tokio::spawn({
        let s = spy.clone();
        async move { s.run().await }
    });
    spy.connect(gossip_addr).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let worker_config = WorkerConfig {
        gossip_addr: "127.0.0.1:0".parse().unwrap(),
        coordinator_addr: gossip_addr,
        rpc_url: RPC_URL.to_string(),
        private_key: ANVIL_ACCOUNT_1_KEY.to_string(),
        registry_address: reg_addr,
    };
    let worker = Worker::new(worker_config).await.unwrap();
    tokio::spawn({
        let g = worker.gossip.clone();
        async move { g.run().await }
    });
    worker.start().await.unwrap();
    tokio::spawn({
        let w = worker.clone();
        async move { w.run().await }
    });
    sleep(Duration::from_millis(200)).await;

    let job_id = JobId([0xbb; 32]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let deadline = now + 300;
    let payload = 5u64.to_le_bytes().to_vec();

    submit_job_onchain(reg_addr, job_id.0, &payload, deadline, reward).await;

    let job = make_fib_job(job_id, 5, reward);
    coord_state.jobs.insert(job_id, job.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job))
        .await
        .unwrap();

    let completed_msg = timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(envelope) = spy_rx.recv().await {
                if let GossipMessage::JobCompleted {
                    job_id: jid,
                    tx_hash,
                } = envelope.payload
                {
                    if jid == job_id {
                        return tx_hash;
                    }
                }
            }
        }
    })
    .await
    .expect("timeout waiting for JobCompleted gossip message");

    assert_ne!(completed_msg.0, [0u8; 32], "tx_hash should be non-zero");
}
