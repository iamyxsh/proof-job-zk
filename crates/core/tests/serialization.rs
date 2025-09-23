use std::collections::HashMap;

use alloy_primitives::Address;
use bincode::config;
use proof_core::{
    enums::{GossipMessage, JobStatus},
    gossip::GossipEnvelope,
    ids::{JobId, MessageId, PeerId},
    job::Job,
};

#[test]
fn job_id_roundtrips_through_bincode() {
    let id = JobId([0xab; 32]);
    let bytes = bincode::serde::encode_to_vec(&id, config::standard()).unwrap();
    let (recovered, _): (JobId, _) =
        bincode::serde::decode_from_slice(&bytes, config::standard()).unwrap();
    assert_eq!(id, recovered);
}

#[test]
fn job_roundtrips_through_bincode() {
    let job = Job {
        id: JobId([0xaa; 32]),
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![1, 2, 3],
        reward: 1000,
        created_at: 1000,
        deadline: 2000,
    };
    let bytes = bincode::serde::encode_to_vec(&job, config::standard()).unwrap();
    let (recovered, _): (Job, _) =
        bincode::serde::decode_from_slice(&bytes, config::standard()).unwrap();
    assert_eq!(job, recovered);
}

#[test]
fn gossip_envelope_roundtrips_through_bincode() {
    let job = Job {
        id: JobId([0xaa; 32]),
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![1, 2, 3],
        reward: 1000,
        created_at: 1000,
        deadline: 2000,
    };
    let envelope = GossipEnvelope {
        id: MessageId([0x01; 32]),
        origin: PeerId([0x02; 32]),
        ttl: 3,
        payload: GossipMessage::JobAvailable(job),
    };
    let bytes = bincode::serde::encode_to_vec(&envelope, config::standard()).unwrap();
    let (recovered, _): (GossipEnvelope, _) =
        bincode::serde::decode_from_slice(&bytes, config::standard()).unwrap();
    assert_eq!(envelope, recovered);
}

#[test]
fn job_id_compiles_as_hashmap_key() {
    let mut map: HashMap<JobId, Job> = HashMap::new();
    let job = Job {
        id: JobId([0xcc; 32]),
        job_status: JobStatus::Pending,
        owner: Address::ZERO,
        payload: vec![],
        reward: 0,
        created_at: 0,
        deadline: 0,
    };
    map.insert(job.id, job);
    assert_eq!(map.len(), 1);
}
