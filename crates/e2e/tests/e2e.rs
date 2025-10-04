use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

const ANVIL_RPC: &str = "http://127.0.0.1:8545";
const COORDINATOR_HTTP: &str = "http://127.0.0.1:8080";

const ANVIL_ACCOUNT_0: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_ACCOUNT_1: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const ANVIL_ACCOUNT_2: &str = "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a";

struct ProcessGuard(Child);

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn full_e2e_success_criteria() {
    if std::env::var("RUN_E2E").is_err() {
        eprintln!("Skipping E2E test. Set RUN_E2E=1 to run.");
        return;
    }

    let registry_address =
        std::env::var("REGISTRY_ADDRESS").expect("REGISTRY_ADDRESS must be set");

    println!("=== Proof Job Marketplace E2E Test ===");
    println!("Registry: {}", registry_address);

    // 1. Verify Anvil is running
    println!("\n[1] Checking Anvil...");
    assert!(check_anvil().await, "Anvil not running on port 8545");
    println!("    ✓ Anvil running");

    // 2. Start Coordinator
    println!("\n[2] Starting Coordinator...");
    let _coordinator = start_coordinator(&registry_address);
    sleep(Duration::from_secs(3)).await;
    assert!(check_coordinator().await, "Coordinator failed to start");
    println!("    ✓ Coordinator running on :8080");

    // 3. Start Worker 1
    println!("\n[3] Starting Worker 1...");
    let _worker1 = start_worker(1, &registry_address);
    sleep(Duration::from_secs(2)).await;
    println!("    ✓ Worker 1 running on gossip :9001");

    // 4. Start Worker 2
    println!("\n[4] Starting Worker 2...");
    let _worker2 = start_worker(2, &registry_address);
    sleep(Duration::from_secs(2)).await;
    println!("    ✓ Worker 2 running on gossip :9002");

    // Give workers time to connect to coordinator
    sleep(Duration::from_secs(2)).await;

    // 5. Submit job via HTTP
    println!("\n[5] Submitting job via HTTP...");
    let job_response = submit_job(10, 1_000_000_000_000_000_000).await;
    let job_id = job_response.job_id.clone();
    println!("    ✓ Job submitted: {}", job_id);
    if let Some(ref tx) = job_response.tx_hash {
        println!("    ✓ Submit TX hash: {}", tx);
    }

    // 6. Wait for completion
    println!("\n[6] Waiting for job completion...");
    let max_wait = Duration::from_secs(60);
    let poll_interval = Duration::from_secs(2);
    let mut elapsed = Duration::ZERO;

    let final_status = loop {
        let status = get_job_status(&job_id).await;
        println!("    Status: {:?}", status.state);

        if status.state == "Completed" {
            break status;
        }

        if elapsed >= max_wait {
            panic!("Timeout waiting for job completion after {:?}", max_wait);
        }

        sleep(poll_interval).await;
        elapsed += poll_interval;
    };

    // 7. Verify success criteria
    println!("\n[7] Verifying success criteria...");

    assert_eq!(final_status.state, "Completed");
    let tx_hash = final_status.tx_hash.expect("tx_hash should be present");
    assert!(tx_hash.starts_with("0x"), "tx_hash should start with 0x");
    assert_eq!(tx_hash.len(), 66, "tx_hash should be 32 bytes hex");

    println!("    ✓ Job status: Completed");
    println!("    ✓ TX hash: {}", tx_hash);

    println!("\n=== SUCCESS ===");
    println!("Proof Job Marketplace E2E test passed!");
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

async fn check_anvil() -> bool {
    let client = reqwest::Client::new();
    client
        .post(ANVIL_RPC)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        }))
        .send()
        .await
        .is_ok()
}

async fn check_coordinator() -> bool {
    let client = reqwest::Client::new();
    client
        .get(format!("{}/health", COORDINATOR_HTTP))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[derive(Debug, serde::Deserialize)]
struct CreateJobResponse {
    job_id: String,
    tx_hash: Option<String>,
}

async fn submit_job(n: u64, reward: u128) -> CreateJobResponse {
    let client = reqwest::Client::new();
    let payload = hex::encode(n.to_le_bytes());

    let res = client
        .post(format!("{}/jobs", COORDINATOR_HTTP))
        .json(&serde_json::json!({
            "payload": format!("0x{}", payload),
            "reward": reward
        }))
        .send()
        .await
        .expect("failed to submit job");

    assert!(
        res.status().is_success(),
        "job submission failed: {}",
        res.status()
    );

    res.json().await.expect("failed to parse response")
}

#[derive(Debug, serde::Deserialize)]
struct JobStatusResponse {
    state: String,
    tx_hash: Option<String>,
}

async fn get_job_status(job_id: &str) -> JobStatusResponse {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{}/jobs/{}/status", COORDINATOR_HTTP, job_id))
        .send()
        .await
        .expect("failed to get job status");

    res.json().await.expect("failed to parse response")
}

fn start_coordinator(registry_address: &str) -> ProcessGuard {
    let child = Command::new("cargo")
        .args(["run", "--release", "-p", "coordinator"])
        .env("REGISTRY_ADDRESS", registry_address)
        .env("RPC_URL", "http://127.0.0.1:8545")
        .env("PRIVATE_KEY", ANVIL_ACCOUNT_0)
        .env(
            "RUST_LOG",
            "coordinator=info,indexer=info,gossip=warn",
        )
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to start coordinator");
    ProcessGuard(child)
}

fn start_worker(id: u8, registry_address: &str) -> ProcessGuard {
    let gossip_port = 9000 + id as u16;
    let private_key = match id {
        1 => ANVIL_ACCOUNT_1,
        2 => ANVIL_ACCOUNT_2,
        _ => panic!("invalid worker id"),
    };

    let child = Command::new("cargo")
        .args(["run", "--release", "-p", "worker"])
        .env("REGISTRY_ADDRESS", registry_address)
        .env("RPC_URL", "http://127.0.0.1:8545")
        .env("GOSSIP_PORT", gossip_port.to_string())
        .env("PRIVATE_KEY", private_key)
        .env("COORDINATOR_GOSSIP", "127.0.0.1:9000")
        .env("RUST_LOG", "worker=info,prover=info,gossip=warn")
        .env("RISC0_DEV_MODE", "1")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to start worker");
    ProcessGuard(child)
}
