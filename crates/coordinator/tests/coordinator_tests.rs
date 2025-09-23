use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use coordinator::api::{self, CreateJobResponse};
use coordinator::setup_test_coordinator;
use gossip::gossip::{GossipConfig, GossipNode};
use http_body_util::BodyExt;
use proof_core::enums::GossipMessage;
use proof_core::ids::JobId;
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
    }
}
