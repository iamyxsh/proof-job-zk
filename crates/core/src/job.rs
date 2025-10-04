use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

use crate::{enums::JobStatus, ids::JobId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub job_status: JobStatus,
    pub owner: Address,
    pub payload: Vec<u8>,
    pub reward: u128,
    pub created_at: u64,
    pub deadline: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobInput {
    pub job_id: JobId,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutput {
    pub job_id: JobId,
    pub payload_hash: [u8; 32],
    pub result: Vec<u8>,
}
