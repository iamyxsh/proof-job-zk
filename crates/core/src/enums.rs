use serde::{Deserialize, Serialize};

use crate::{
    ids::{JobId, PeerId, TxHash},
    job::Job,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Assigned { worker: PeerId, assigned_at: u64 },
    Proving,
    Completed { tx_hash: TxHash },
    Failed { reason: Vec<u8> },
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GossipMessage {
    JobAvailable(Job),
    ClaimJob { job_id: JobId, worker_id: PeerId },
    ClaimAccepted { job_id: JobId, worker_id: PeerId },
    ClaimRejected { job_id: JobId, reason: String },
    JobCompleted { job_id: JobId, tx_hash: TxHash },
}
