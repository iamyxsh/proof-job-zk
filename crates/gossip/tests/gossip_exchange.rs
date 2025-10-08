use std::time::Duration;

use alloy_primitives::Address;
use ed25519_dalek::SigningKey;
use gossip::gossip::{GossipConfig, GossipNode};
use proof_core::{
    enums::{GossipMessage, JobStatus},
    gossip::GossipEnvelope,
    ids::{JobId, MessageId, PeerId},
    job::Job,
};
use tokio::time::{sleep, timeout};

fn make_test_job() -> Job {
    Job {
        id: JobId([0xaa; 32]),
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![1, 2, 3],
        reward: 1000,
        created_at: 1000,
        deadline: 2000,
    }
}

fn make_signed_envelope() -> GossipEnvelope {
    let key = SigningKey::generate(&mut rand::thread_rng());
    let origin = PeerId(key.verifying_key().to_bytes());
    let mut envelope = GossipEnvelope {
        id: MessageId([0x01; 32]),
        origin,
        ttl: 3,
        payload: GossipMessage::JobAvailable(make_test_job()),
        signature: [0u8; 64],
    };
    envelope.sign(&key);
    envelope
}

fn config() -> GossipConfig {
    GossipConfig::new("127.0.0.1:0".parse().unwrap())
}

#[tokio::test]
async fn two_nodes_can_exchange_message() {
    let node_a = GossipNode::new(config()).await.unwrap();
    let addr_a = node_a.local_addr().unwrap();
    let mut rx_a = node_a.subscribe();
    tokio::spawn({
        let node = node_a.clone();
        async move { node.run().await }
    });

    let node_b = GossipNode::new(config()).await.unwrap();
    tokio::spawn({
        let node = node_b.clone();
        async move { node.run().await }
    });

    node_b.connect(addr_a).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let envelope = make_signed_envelope();
    node_b.send(envelope.clone()).await.unwrap();

    let received = timeout(Duration::from_secs(1), rx_a.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(received.id, envelope.id);
}

#[tokio::test]
async fn message_propagates_through_relay() {
    let node_a = GossipNode::new(config()).await.unwrap();
    tokio::spawn({ let n = node_a.clone(); async move { n.run().await } });

    let node_b = GossipNode::new(config()).await.unwrap();
    let addr_b = node_b.local_addr().unwrap();
    tokio::spawn({ let n = node_b.clone(); async move { n.run().await } });

    let node_c = GossipNode::new(config()).await.unwrap();
    let mut rx_c = node_c.subscribe();
    tokio::spawn({ let n = node_c.clone(); async move { n.run().await } });

    node_a.connect(addr_b).await.unwrap();
    node_c.connect(addr_b).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let job = make_test_job();
    node_a
        .broadcast(GossipMessage::JobAvailable(job.clone()))
        .await
        .unwrap();

    let received = timeout(Duration::from_secs(1), rx_c.recv())
        .await
        .unwrap()
        .unwrap();

    match &received.payload {
        GossipMessage::JobAvailable(j) => assert_eq!(j.id, job.id),
        _ => panic!("expected JobAvailable message"),
    }

    assert_eq!(received.ttl, 2);
}

#[tokio::test]
async fn duplicate_messages_are_dropped() {
    let node_a = GossipNode::new(config()).await.unwrap();
    tokio::spawn({ let n = node_a.clone(); async move { n.run().await } });

    let node_b = GossipNode::new(config()).await.unwrap();
    let addr_b = node_b.local_addr().unwrap();
    let mut rx_b = node_b.subscribe();
    tokio::spawn({ let n = node_b.clone(); async move { n.run().await } });

    node_a.connect(addr_b).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let envelope = make_signed_envelope();
    node_a.send(envelope.clone()).await.unwrap();
    node_a.send(envelope.clone()).await.unwrap();

    let _first = timeout(Duration::from_millis(500), rx_b.recv())
        .await
        .unwrap()
        .unwrap();
    let second = timeout(Duration::from_millis(500), rx_b.recv()).await;

    assert!(second.is_err(), "should not receive duplicate");
}

#[tokio::test]
async fn ttl_one_not_forwarded() {
    let node_a = GossipNode::new(config()).await.unwrap();
    tokio::spawn({ let n = node_a.clone(); async move { n.run().await } });

    let node_b = GossipNode::new(config()).await.unwrap();
    let addr_b = node_b.local_addr().unwrap();
    let mut rx_b = node_b.subscribe();
    tokio::spawn({ let n = node_b.clone(); async move { n.run().await } });

    let node_c = GossipNode::new(config()).await.unwrap();
    let mut rx_c = node_c.subscribe();
    tokio::spawn({ let n = node_c.clone(); async move { n.run().await } });

    node_a.connect(addr_b).await.unwrap();
    node_c.connect(addr_b).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let key = SigningKey::generate(&mut rand::thread_rng());
    let origin = PeerId(key.verifying_key().to_bytes());
    let mut envelope = GossipEnvelope {
        id: MessageId([0x01; 32]),
        origin,
        ttl: 1,
        payload: GossipMessage::JobAvailable(make_test_job()),
        signature: [0u8; 64],
    };
    envelope.sign(&key);
    node_a.send(envelope).await.unwrap();

    let _ = timeout(Duration::from_millis(500), rx_b.recv())
        .await
        .unwrap();

    let result = timeout(Duration::from_millis(500), rx_c.recv()).await;
    assert!(result.is_err(), "TTL=1 message should not reach C");
}

#[tokio::test]
async fn unsigned_messages_are_rejected() {
    let node_a = GossipNode::new(config()).await.unwrap();
    let addr_a = node_a.local_addr().unwrap();
    let mut rx_a = node_a.subscribe();
    tokio::spawn({
        let n = node_a.clone();
        async move { n.run().await }
    });

    let node_b = GossipNode::new(config()).await.unwrap();
    tokio::spawn({
        let n = node_b.clone();
        async move { n.run().await }
    });

    node_b.connect(addr_a).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let unsigned = GossipEnvelope {
        id: MessageId([0x02; 32]),
        origin: PeerId([0xBB; 32]),
        ttl: 3,
        payload: GossipMessage::JobAvailable(make_test_job()),
        signature: [0u8; 64],
    };
    node_b.send(unsigned).await.unwrap();

    let result = timeout(Duration::from_millis(500), rx_a.recv()).await;
    assert!(result.is_err(), "unsigned message should be rejected");
}
