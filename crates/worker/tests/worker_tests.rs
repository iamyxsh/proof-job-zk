use std::time::Duration;

use alloy_primitives::Address;
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::setup_test_coordinator;
use proof_core::enums::{GossipMessage, JobStatus};
use proof_core::ids::JobId;
use proof_core::job::Job;
use tokio::time::sleep;
use worker::{Worker, WorkerConfig, WorkerStatus};

const ANVIL_ACCOUNT_1_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

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
        rpc_url: "http://127.0.0.1:8545".to_string(),
        private_key: ANVIL_ACCOUNT_1_KEY.to_string(),
        registry_address: Address::ZERO,
    };
    Worker::new(config).await.unwrap()
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

    sleep(Duration::from_secs(5)).await;

    let coord_job = coord_state.jobs.get(&job_id).unwrap();
    match coord_job.job_status {
        JobStatus::Assigned { worker: w, .. } => {
            assert_eq!(w, worker.peer_id);
        }
        ref other => panic!("coordinator job not assigned, got {:?}", other),
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

    let job_id_1 = JobId([0xaa; 32]);
    let job_1 = make_fib_job(job_id_1, 10);
    coord_state.jobs.insert(job_id_1, job_1.clone());

    let job_id_2 = JobId([0xbb; 32]);
    let job_2 = make_fib_job(job_id_2, 5);
    coord_state.jobs.insert(job_id_2, job_2.clone());

    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job_1))
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job_2))
        .await
        .unwrap();

    sleep(Duration::from_secs(5)).await;

    let coord_job_1 = coord_state.jobs.get(&job_id_1).unwrap();
    assert!(
        matches!(coord_job_1.job_status, JobStatus::Assigned { .. }),
        "job 1 should be assigned, got {:?}",
        coord_job_1.job_status
    );

    let coord_job_2 = coord_state.jobs.get(&job_id_2).unwrap();
    assert!(
        matches!(coord_job_2.job_status, JobStatus::Pending),
        "job 2 should still be Pending (worker ignored it), got {:?}",
        coord_job_2.job_status
    );
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

    sleep(Duration::from_secs(1)).await;

    let coord_job = coord_state.jobs.get(&job_id).unwrap();
    let assigned_peer = match coord_job.job_status {
        JobStatus::Assigned { worker, .. } => worker,
        ref other => panic!("expected Assigned, got {:?}", other),
    };

    assert!(
        assigned_peer == worker1.peer_id || assigned_peer == worker2.peer_id,
        "assigned worker should be one of our workers"
    );

    let (_winner, loser) = if assigned_peer == worker1.peer_id {
        (&worker1, &worker2)
    } else {
        (&worker2, &worker1)
    };

    let loser_status = loser.status.read().await;
    assert!(
        matches!(&*loser_status, WorkerStatus::Idle),
        "losing worker should be Idle, got {:?}",
        *loser_status
    );
}
