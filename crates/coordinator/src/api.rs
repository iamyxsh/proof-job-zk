use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::Address;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::extract::Path;
use axum::routing::{get, post};
use axum::{Json, Router};
use proof_core::enums::{GossipMessage, JobStatus};
use proof_core::ids::JobId;
use proof_core::job::Job;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/jobs", post(create_job))
        .route("/jobs/:id", get(get_job))
        .route("/jobs/:id/status", get(get_job_status))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
pub struct CreateJobRequest {
    pub payload: String,
    pub reward: u128,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CreateJobResponse {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
}

async fn create_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let payload = hex::decode(req.payload.trim_start_matches("0x"))
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid hex payload: {e}")))?;

    let owner = req
        .owner
        .map(|s| s.trim_start_matches("0x").parse::<Address>())
        .transpose()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid owner address: {e}")))?
        .unwrap_or(Address::ZERO);

    let nonce: [u8; 8] = rand::thread_rng().gen();
    let job_id = generate_job_id(&owner, &payload, &nonce);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let deadline = now + state.config.default_deadline_secs;

    let mut submit_tx_hash: Option<String> = None;
    if let Some(ref contract) = state.contract {
        let tx = contract
            .submit_job(job_id.0, &payload, deadline, req.reward)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("on-chain submission failed: {e}"),
                )
            })?;
        submit_tx_hash = Some(format!("0x{}", hex::encode(tx.0)));
    }

    let job = Job {
        id: job_id,
        job_status: JobStatus::Pending,
        owner,
        payload,
        reward: req.reward,
        created_at: now,
        deadline,
    };

    state.jobs.insert(job_id, job.clone());

    state
        .gossip
        .broadcast(GossipMessage::JobAvailable(job))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("broadcast failed: {e}")))?;

    Ok(Json(CreateJobResponse {
        job_id: format!("0x{}", hex::encode(job_id.0)),
        tx_hash: submit_tx_hash,
    }))
}

fn generate_job_id(owner: &Address, payload: &[u8], nonce: &[u8]) -> JobId {
    let mut hasher = Sha256::new();
    hasher.update(owner.as_slice());
    hasher.update(payload);
    hasher.update(nonce);
    JobId(hasher.finalize().into())
}

#[derive(Serialize, Deserialize)]
pub struct JobResponse {
    pub id: String,
    pub owner: String,
    pub payload: String,
    pub reward: u128,
    pub created_at: u64,
    pub deadline: u64,
    pub status: JobStatusResponse,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "state")]
pub enum JobStatusResponse {
    Pending,
    Assigned {
        worker: String,
        assigned_at: u64,
    },
    Proving,
    Completed {
        tx_hash: String,
    },
    Failed {
        reason: String,
    },
    Expired,
}

impl From<&JobStatus> for JobStatusResponse {
    fn from(status: &JobStatus) -> Self {
        match status {
            JobStatus::Pending => JobStatusResponse::Pending,
            JobStatus::Assigned {
                worker,
                assigned_at,
            } => JobStatusResponse::Assigned {
                worker: hex::encode(worker.0),
                assigned_at: *assigned_at,
            },
            JobStatus::Proving => JobStatusResponse::Proving,
            JobStatus::Completed { tx_hash } => JobStatusResponse::Completed {
                tx_hash: format!("0x{}", hex::encode(tx_hash.0)),
            },
            JobStatus::Failed { reason } => JobStatusResponse::Failed {
                reason: String::from_utf8_lossy(reason).to_string(),
            },
            JobStatus::Expired => JobStatusResponse::Expired,
        }
    }
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, (StatusCode, String)> {
    let job_id = parse_job_id(&id)?;

    let job = state
        .jobs
        .get(&job_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "job not found".to_string()))?;

    Ok(Json(JobResponse {
        id: format!("0x{}", hex::encode(job.id.0)),
        owner: format!("{}", job.owner),
        payload: format!("0x{}", hex::encode(&job.payload)),
        reward: job.reward,
        created_at: job.created_at,
        deadline: job.deadline,
        status: JobStatusResponse::from(&job.job_status),
    }))
}

async fn get_job_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<JobStatusResponse>, (StatusCode, String)> {
    let job_id = parse_job_id(&id)?;

    let job = state
        .jobs
        .get(&job_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "job not found".to_string()))?;

    Ok(Json(JobStatusResponse::from(&job.job_status)))
}

fn parse_job_id(id: &str) -> Result<JobId, (StatusCode, String)> {
    let hex_str = id.trim_start_matches("0x");
    let bytes = hex::decode(hex_str)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid job id: {e}")))?;

    if bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            "job id must be 32 bytes".to_string(),
        ));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(JobId(arr))
}
