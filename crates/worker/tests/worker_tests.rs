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

async fn setup_test_worker(
    coordinator_addr: std::net::SocketAddr,
) -> std::sync::Arc<Worker> {
    let config = WorkerConfig {
        gossip_addr: "127.0.0.1:0".parse().unwrap(),
        coordinator_addr,
    };
    Worker::new(config).await.unwrap()
}

#[tokio::test]
async fn worker_claims_available_job() {
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

    sleep(Duration::from_millis(500)).await;

    let status = worker.status.read().await;
    match &*status {
        WorkerStatus::Assigned { job } => {
            assert_eq!(job.id, job_id);
        }
        other => panic!("expected Assigned, got {:?}", other),
    }
    drop(status);

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

    // First job — worker should claim and get assigned
    let job_id_1 = JobId([0xaa; 32]);
    let job_1 = make_test_job(job_id_1);
    coord_state.jobs.insert(job_id_1, job_1.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job_1))
        .await
        .unwrap();

    sleep(Duration::from_millis(500)).await;

    // Verify assigned to first job
    {
        let status = worker.status.read().await;
        assert!(
            matches!(&*status, WorkerStatus::Assigned { job } if job.id == job_id_1),
            "expected Assigned to job 1"
        );
    }

    // Second job — worker should ignore it
    let job_id_2 = JobId([0xbb; 32]);
    let job_2 = make_test_job(job_id_2);
    coord_state.jobs.insert(job_id_2, job_2.clone());
    coord_state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job_2))
        .await
        .unwrap();

    sleep(Duration::from_millis(300)).await;

    let status = worker.status.read().await;
    match &*status {
        WorkerStatus::Assigned { job } => {
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

    sleep(Duration::from_millis(1000)).await;

    let status1 = worker1.status.read().await;
    let status2 = worker2.status.read().await;

    let assigned_count = [&*status1, &*status2]
        .iter()
        .filter(|s| matches!(s, WorkerStatus::Assigned { .. }))
        .count();

    let idle_count = [&*status1, &*status2]
        .iter()
        .filter(|s| matches!(s, WorkerStatus::Idle))
        .count();

    assert_eq!(assigned_count, 1, "exactly one worker should be assigned");
    assert_eq!(idle_count, 1, "exactly one worker should be idle");
}
