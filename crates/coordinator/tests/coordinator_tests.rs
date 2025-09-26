use std::time::Duration;

use alloy_primitives::Address;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use coordinator::api::{self, CreateJobResponse};
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::setup_test_coordinator;
use gossip::gossip::{GossipConfig, GossipNode};
use http_body_util::BodyExt;
use proof_core::enums::{GossipMessage, JobStatus};
use proof_core::ids::{JobId, PeerId};
use proof_core::job::Job;
use tokio::time::{sleep, timeout};
use tower::ServiceExt;

#[tokio::test]
async fn health_check() {
    let state = setup_test_coordinator().await;
    let app = api::router(state);

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_job_returns_id() {
    let state = setup_test_coordinator().await;
    tokio::spawn({ let g = state.gossip.clone(); async move { g.run().await } });

    let app = api::router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"payload": "0x1234", "reward": 1000}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CreateJobResponse = serde_json::from_slice(&body).unwrap();

    assert!(resp.job_id.starts_with("0x"));
    assert_eq!(resp.job_id.len(), 66);
}

#[tokio::test]
async fn create_job_stores_in_state() {
    let state = setup_test_coordinator().await;
    tokio::spawn({ let g = state.gossip.clone(); async move { g.run().await } });

    let app = api::router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"payload": "0xabcd", "reward": 2000}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: CreateJobResponse = serde_json::from_slice(&body).unwrap();

    let id_bytes = hex::decode(resp.job_id.trim_start_matches("0x")).unwrap();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&id_bytes);
    let job_id = JobId(arr);

    assert!(state.jobs.contains_key(&job_id));
    let job = state.jobs.get(&job_id).unwrap();
    assert_eq!(job.reward, 2000);
    assert_eq!(job.payload, vec![0xab, 0xcd]);
}

#[tokio::test]
async fn job_submission_broadcasts_to_gossip_peer() {
    let state = setup_test_coordinator().await;
    let gossip_addr = state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = state.gossip.clone(); async move { g.run().await } });

    let spy_config = GossipConfig::new("127.0.0.1:0".parse().unwrap());
    let spy = GossipNode::new(spy_config).await.unwrap();
    let mut spy_rx = spy.subscribe();
    tokio::spawn({ let s = spy.clone(); async move { s.run().await } });

    spy.connect(gossip_addr).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let app = api::router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"payload": "0xdeadbeef", "reward": 5000}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let received = timeout(Duration::from_secs(2), spy_rx.recv())
        .await
        .expect("timeout waiting for gossip")
        .expect("channel closed");

    match received.payload {
        GossipMessage::JobAvailable(job) => {
            assert_eq!(job.reward, 5000);
            assert_eq!(job.payload, vec![0xde, 0xad, 0xbe, 0xef]);
        }
        _ => panic!("expected JobAvailable message"),
    }
}

fn make_test_job(job_id: JobId) -> Job {
    Job {
        id: job_id,
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![1, 2, 3],
        reward: 1000,
        created_at: 1000,
        deadline: 2000,
    }
}

#[tokio::test]
async fn coordinator_accepts_first_claim() {
    let state = setup_test_coordinator().await;
    let gossip_addr = state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(state.clone(), state.gossip.subscribe()));

    let spy_config = GossipConfig::new("127.0.0.1:0".parse().unwrap());
    let spy = GossipNode::new(spy_config).await.unwrap();
    let mut spy_rx = spy.subscribe();
    tokio::spawn({ let s = spy.clone(); async move { s.run().await } });

    spy.connect(gossip_addr).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let job_id = JobId([0xaa; 32]);
    state.jobs.insert(job_id, make_test_job(job_id));

    let worker_id = PeerId([0x42; 32]);
    spy.broadcast(GossipMessage::ClaimJob { job_id, worker_id })
        .await
        .unwrap();

    let response = timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(envelope) = spy_rx.recv().await {
                match envelope.payload {
                    GossipMessage::ClaimAccepted {
                        job_id: jid,
                        worker_id: wid,
                    } if jid == job_id => return (jid, wid),
                    _ => continue,
                }
            }
        }
    })
    .await
    .expect("timeout waiting for ClaimAccepted");

    assert_eq!(response.0, job_id);
    assert_eq!(response.1, worker_id);

    let job = state.jobs.get(&job_id).unwrap();
    match job.job_status {
        JobStatus::Assigned { worker, .. } => assert_eq!(worker, worker_id),
        _ => panic!("expected Assigned status"),
    }
}

#[tokio::test]
async fn coordinator_rejects_second_claim() {
    let state = setup_test_coordinator().await;
    let gossip_addr = state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(state.clone(), state.gossip.subscribe()));

    let spy_config = GossipConfig::new("127.0.0.1:0".parse().unwrap());
    let spy = GossipNode::new(spy_config).await.unwrap();
    let mut spy_rx = spy.subscribe();
    tokio::spawn({ let s = spy.clone(); async move { s.run().await } });

    spy.connect(gossip_addr).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let job_id = JobId([0xbb; 32]);
    state.jobs.insert(job_id, make_test_job(job_id));

    let worker_1 = PeerId([0x01; 32]);
    spy.broadcast(GossipMessage::ClaimJob {
        job_id,
        worker_id: worker_1,
    })
    .await
    .unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(envelope) = spy_rx.recv().await {
                if let GossipMessage::ClaimAccepted { job_id: jid, .. } = envelope.payload {
                    if jid == job_id {
                        return;
                    }
                }
            }
        }
    })
    .await
    .expect("timeout waiting for ClaimAccepted");

    let worker_2 = PeerId([0x02; 32]);
    spy.broadcast(GossipMessage::ClaimJob {
        job_id,
        worker_id: worker_2,
    })
    .await
    .unwrap();

    let reason = timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(envelope) = spy_rx.recv().await {
                if let GossipMessage::ClaimRejected {
                    job_id: jid,
                    reason,
                } = envelope.payload
                {
                    if jid == job_id {
                        return reason;
                    }
                }
            }
        }
    })
    .await
    .expect("timeout waiting for ClaimRejected");

    assert!(reason.contains("already assigned"));
}

#[tokio::test]
async fn coordinator_rejects_unknown_job() {
    let state = setup_test_coordinator().await;
    let gossip_addr = state.gossip.local_addr().unwrap();
    tokio::spawn({ let g = state.gossip.clone(); async move { g.run().await } });
    tokio::spawn(run_gossip_handler(state.clone(), state.gossip.subscribe()));

    let spy_config = GossipConfig::new("127.0.0.1:0".parse().unwrap());
    let spy = GossipNode::new(spy_config).await.unwrap();
    let mut spy_rx = spy.subscribe();
    tokio::spawn({ let s = spy.clone(); async move { s.run().await } });

    spy.connect(gossip_addr).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let fake_job_id = JobId([0xff; 32]);
    let worker_id = PeerId([0x42; 32]);
    spy.broadcast(GossipMessage::ClaimJob {
        job_id: fake_job_id,
        worker_id,
    })
    .await
    .unwrap();

    let reason = timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(envelope) = spy_rx.recv().await {
                if let GossipMessage::ClaimRejected {
                    job_id: jid,
                    reason,
                } = envelope.payload
                {
                    if jid == fake_job_id {
                        return reason;
                    }
                }
            }
        }
    })
    .await
    .expect("timeout waiting for ClaimRejected");

    assert!(reason.contains("not found"));
}
