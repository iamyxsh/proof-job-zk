use std::time::Duration;

use alloy_primitives::Address;
use gossip::gossip::GossipNode;
use proof_core::{
    enums::{GossipMessage, JobStatus},
    gossip::GossipEnvelope,
    ids::{JobId, MessageId, PeerId},
    job::Job,
};

fn make_test_job() -> Job {
    Job {
        id: JobId([0xaa; 32]),
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![1, 2, 3],
        created_at: 1000,
        deadline: 2000,
    }
}

#[tokio::test]
async fn two_nodes_can_exchange_message() {
    // Start node A on port 9000
    let node_a = GossipNode::new("127.0.0.1:9000".parse().unwrap())
        .await
        .unwrap();
    let mut rx_a = node_a.subscribe();
    tokio::spawn({
        let node = node_a.clone();
        async move { node.run().await }
    });

    // Start node B on port 9001
    let node_b = GossipNode::new("127.0.0.1:9001".parse().unwrap())
        .await
        .unwrap();
    let _rx_b = node_b.subscribe();
    tokio::spawn({
        let node = node_b.clone();
        async move { node.run().await }
    });

    // B connects to A
    node_b
        .connect("127.0.0.1:9000".parse().unwrap())
        .await
        .unwrap();

    // Give connection time to establish
    tokio::time::sleep(Duration::from_millis(100)).await;

    // B sends a message
    let job = make_test_job();
    let envelope = GossipEnvelope {
        id: MessageId([0x01; 32]),
        origin: PeerId([0xBB; 32]),
        ttl: 3,
        payload: GossipMessage::JobAvailable(job.clone()),
    };
    node_b.send(envelope.clone()).await.unwrap();

    // A receives it
    let received = tokio::time::timeout(Duration::from_secs(1), rx_a.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(received.id, envelope.id);
}
