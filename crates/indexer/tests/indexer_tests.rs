use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use contract_client::ContractClient;
use dashmap::DashMap;
use indexer::{create_indexer, IndexerConfig};
use proof_core::enums::JobStatus;
use proof_core::ids::JobId;
use proof_core::job::Job;
use tokio::time::sleep;

fn rpc_url() -> String {
    std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string())
}

fn registry_address() -> Option<Address> {
    std::env::var("REGISTRY_ADDRESS")
        .ok()
        .map(|s| s.parse().expect("invalid REGISTRY_ADDRESS"))
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

const ANVIL_KEY_0: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_KEY_1: &str = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

#[tokio::test]
async fn indexer_detects_job_completed() {
    let Some(addr) = registry_address() else {
        eprintln!("REGISTRY_ADDRESS not set, skipping indexer_detects_job_completed");
        return;
    };

    let rpc = rpc_url();
    let jobs: Arc<DashMap<JobId, Job>> = Arc::new(DashMap::new());

    let job_id = JobId([0xaa; 32]);
    let now = unix_timestamp();
    let deadline = now + 300;

    let job = Job {
        id: job_id,
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![10],
        reward: 1_000_000_000_000_000_000,
        created_at: now,
        deadline,
    };
    jobs.insert(job_id, job);

    let indexer_config = IndexerConfig {
        rpc_url: rpc.clone(),
        registry_address: addr,
        poll_interval: Duration::from_millis(500),
        start_block: None,
    };
    let indexer = create_indexer(indexer_config, jobs.clone()).await.unwrap();
    tokio::spawn(indexer.run());

    let client = ContractClient::new(&rpc, addr, ANVIL_KEY_0).unwrap();
    client
        .submit_job(job_id.0, &[10], deadline, 1_000_000_000_000_000_000)
        .await
        .expect("submit_job failed");

    let worker_client = ContractClient::new(&rpc, addr, ANVIL_KEY_1).unwrap();
    worker_client
        .submit_proof(job_id.0, &[55], &[], [0u8; 32])
        .await
        .expect("submit_proof failed");

    sleep(Duration::from_secs(3)).await;

    let job = jobs.get(&job_id).unwrap();
    match &job.job_status {
        JobStatus::Completed { tx_hash } => {
            assert_ne!(tx_hash.0, [0u8; 32], "tx_hash should not be zero");
            println!(
                "OK: job completed with tx_hash 0x{}",
                hex::encode(tx_hash.0)
            );
        }
        other => panic!("expected Completed, got {:?}", other),
    }
}

#[tokio::test]
async fn indexer_handles_unknown_job() {
    let Some(addr) = registry_address() else {
        eprintln!("REGISTRY_ADDRESS not set, skipping indexer_handles_unknown_job");
        return;
    };

    let rpc = rpc_url();
    let jobs: Arc<DashMap<JobId, Job>> = Arc::new(DashMap::new());

    let indexer_config = IndexerConfig {
        rpc_url: rpc.clone(),
        registry_address: addr,
        poll_interval: Duration::from_millis(500),
        start_block: None,
    };
    let indexer = create_indexer(indexer_config, jobs.clone()).await.unwrap();
    tokio::spawn(indexer.run());

    let job_id = [0xbb; 32];
    let deadline = unix_timestamp() + 300;

    let client = ContractClient::new(&rpc, addr, ANVIL_KEY_0).unwrap();
    client
        .submit_job(job_id, &[20], deadline, 1_000_000_000_000_000_000)
        .await
        .expect("submit_job failed");

    let worker_client = ContractClient::new(&rpc, addr, ANVIL_KEY_1).unwrap();
    worker_client
        .submit_proof(job_id, &[66], &[], [0u8; 32])
        .await
        .expect("submit_proof failed");

    sleep(Duration::from_secs(3)).await;

    assert!(
        !jobs.contains_key(&JobId(job_id)),
        "unknown job should not be added to state"
    );
    println!("OK: unknown job was not added to state");
}
