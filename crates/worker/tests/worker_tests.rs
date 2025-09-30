use std::time::Duration;

use alloy_primitives::Address;
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::setup_test_coordinator;
use proof_core::enums::{GossipMessage, JobStatus};
use proof_core::ids::JobId;
use proof_core::job::Job;
use tokio::time::sleep;
use worker::{Worker, WorkerConfig, WorkerStatus};

fn make_test_job(job_id: JobId) -> Job {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Job {
        id: job_id,
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![1, 2, 3],
        reward: 1000,
        created_at: now,
        deadline: now + 300,
    }
}

fn make_fib_job(job_id: JobId, n: u64) -> Job {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Job {
        id: job_id,
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: n.to_le_bytes().to_vec(),
        reward: 1000,
        created_at: now,
        deadline: now + 300,
    }
}

async fn setup_test_worker(
    coordinator_addr: std::net::SocketAddr,
) -> std::sync::Arc<Worker> {
    let config = WorkerConfig {
        gossip_addr: "127.0.0.1:0".parse().unwrap(),
        coordinator_addr,
    };
    Worker::new(config).await.unwrap()
}

/// Helper: check if worker is in a "busy" state (not Idle, not Claiming)
fn is_proving_or_ready(status: &WorkerStatus) -> bool {
    matches!(status, WorkerStatus::Proving { .. } | WorkerStatus::ProofReady { .. })
}

#[tokio::test]
async fn worker_claims_and_proves_job() {
    let coord_state = setup_test_coordinator().await;
    let gossip_addr = coord_state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = coord_state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(coord_state.clone(), coord_state.gossip.subscribe()));

    let worker = setup_test_worker(gossip_addr).await;
    tokio::spawn({ let g = worker.gossip.clone(); async move { g.run().await } });
    worker.start().await.unwrap();
    tokio::spawn({ let w = worker.clone(); async move { w.run().await } });
    sleep(Duration::from_millis(100)).await;

    let job_id = JobId([0xaa; 32]);
    let job = make_test_job(job_id);
    coord_state.jobs.insert(job_id, job.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job))
        .await
        .unwrap();

    // Wait for claim + accept + proving (dev mode proving is fast)
    sleep(Duration::from_secs(3)).await;

    let status = worker.status.read().await;
    assert!(
        is_proving_or_ready(&status),
        "expected Proving or ProofReady, got {:?}",
        *status
    );
    drop(status);

    // Coordinator should have assigned the job
    let coord_job = coord_state.jobs.get(&job_id).unwrap();
    match coord_job.job_status {
        JobStatus::Assigned { worker: w, .. } => {
            assert_eq!(w, worker.peer_id);
        }
        ref other => panic!("coordinator job not assigned, got {:?}", other),
    }
}

#[tokio::test]
async fn worker_proves_after_claim_accepted() {
    let coord_state = setup_test_coordinator().await;
    let gossip_addr = coord_state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = coord_state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(coord_state.clone(), coord_state.gossip.subscribe()));

    let worker = setup_test_worker(gossip_addr).await;
    tokio::spawn({ let g = worker.gossip.clone(); async move { g.run().await } });
    worker.start().await.unwrap();
    tokio::spawn({ let w = worker.clone(); async move { w.run().await } });
    sleep(Duration::from_millis(100)).await;

    // Create job: compute fib(10)
    let job_id = JobId([0xaa; 32]);
    let job = make_fib_job(job_id, 10);
    coord_state.jobs.insert(job_id, job.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job))
        .await
        .unwrap();

    // Wait for claim + accept + proving to complete
    sleep(Duration::from_secs(5)).await;

    // Worker should be in ProofReady state
    let status = worker.status.read().await;
    match &*status {
        WorkerStatus::ProofReady { job: j, bundle } => {
            assert_eq!(j.id, job_id);

            // Verify proof result: fib(10) = 55
            let result = u64::from_le_bytes(
                bundle.output.result[..8].try_into().unwrap(),
            );
            assert_eq!(result, 55);

            // Image ID should be set
            assert_eq!(bundle.image_id, prover::GUEST_ID);

            // Receipt bytes should be non-empty
            assert!(!bundle.receipt_bytes.is_empty());
        }
        other => panic!("expected ProofReady, got {:?}", other),
    }
}

#[tokio::test]
async fn worker_ignores_job_when_busy() {
    let coord_state = setup_test_coordinator().await;
    let gossip_addr = coord_state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = coord_state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(coord_state.clone(), coord_state.gossip.subscribe()));

    let worker = setup_test_worker(gossip_addr).await;
    tokio::spawn({ let g = worker.gossip.clone(); async move { g.run().await } });
    worker.start().await.unwrap();
    tokio::spawn({ let w = worker.clone(); async move { w.run().await } });
    sleep(Duration::from_millis(100)).await;

    // First job
    let job_id_1 = JobId([0xaa; 32]);
    let job_1 = make_fib_job(job_id_1, 10);
    coord_state.jobs.insert(job_id_1, job_1.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job_1))
        .await
        .unwrap();

    // Wait for claim + proving to start/complete
    sleep(Duration::from_secs(3)).await;

    {
        let status = worker.status.read().await;
        assert!(
            is_proving_or_ready(&status),
            "expected Proving or ProofReady for job 1, got {:?}",
            *status
        );
    }

    // Second job - worker should ignore it
    let job_id_2 = JobId([0xbb; 32]);
    let job_2 = make_fib_job(job_id_2, 5);
    coord_state.jobs.insert(job_id_2, job_2.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job_2))
        .await
        .unwrap();

    sleep(Duration::from_millis(500)).await;

    // Worker should still be on first job
    let status = worker.status.read().await;
    match &*status {
        WorkerStatus::Proving { job } | WorkerStatus::ProofReady { job, .. } => {
            assert_eq!(job.id, job_id_1, "worker should still have first job");
        }
        other => panic!("state changed unexpectedly: {:?}", other),
    }
}

#[tokio::test]
async fn losing_worker_returns_to_idle() {
    let coord_state = setup_test_coordinator().await;
    let gossip_addr = coord_state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = coord_state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(coord_state.clone(), coord_state.gossip.subscribe()));

    let worker1 = setup_test_worker(gossip_addr).await;
    tokio::spawn({ let g = worker1.gossip.clone(); async move { g.run().await } });
    worker1.start().await.unwrap();
    tokio::spawn({ let w = worker1.clone(); async move { w.run().await } });

    let worker2 = setup_test_worker(gossip_addr).await;
    tokio::spawn({ let g = worker2.gossip.clone(); async move { g.run().await } });
    worker2.start().await.unwrap();
    tokio::spawn({ let w = worker2.clone(); async move { w.run().await } });

    sleep(Duration::from_millis(200)).await;

    let job_id = JobId([0xcc; 32]);
    let job = make_test_job(job_id);
    coord_state.jobs.insert(job_id, job.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job))
        .await
        .unwrap();

    sleep(Duration::from_secs(3)).await;

    let status1 = worker1.status.read().await;
    let status2 = worker2.status.read().await;

    // One worker should be proving/proof-ready, the other idle
    let busy_count = [&*status1, &*status2]
        .iter()
        .filter(|s| is_proving_or_ready(s))
        .count();

    let idle_count = [&*status1, &*status2]
        .iter()
        .filter(|s| matches!(s, WorkerStatus::Idle))
        .count();

    assert_eq!(busy_count, 1, "exactly one worker should be proving/ready");
    assert_eq!(idle_count, 1, "exactly one worker should be idle");
}
