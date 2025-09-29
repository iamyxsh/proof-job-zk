use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::Address;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
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

    if let Some(ref contract) = state.contract {
        contract
            .submit_job(job_id.0, &payload, deadline, req.reward)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("on-chain submission failed: {e}"),
                )
            })?;
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
    }))
}

fn generate_job_id(owner: &Address, payload: &[u8], nonce: &[u8]) -> JobId {
    let mut hasher = Sha256::new();
    hasher.update(owner.as_slice());
    hasher.update(payload);
    hasher.update(nonce);
    JobId(hasher.finalize().into())
}
