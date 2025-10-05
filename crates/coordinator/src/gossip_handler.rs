use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use proof_core::enums::{GossipMessage, JobStatus};
use proof_core::gossip::GossipEnvelope;
use proof_core::ids::{JobId, PeerId, TxHash};
use tokio::sync::broadcast;

use crate::AppState;

pub async fn run_gossip_handler(
    state: Arc<AppState>,
    mut rx: broadcast::Receiver<GossipEnvelope>,
) {
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                if let Err(e) = process_message(&state, envelope).await {
                    eprintln!("Error processing gossip message: {}", e);
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("Gossip handler lagged, missed {} messages", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}

async fn process_message(
    state: &AppState,
    envelope: GossipEnvelope,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match envelope.payload {
        GossipMessage::ClaimJob { job_id, worker_id } => {
            handle_claim(state, job_id, worker_id).await
        }
        GossipMessage::JobCompleted { job_id, tx_hash } => {
            handle_job_completed(state, job_id, tx_hash).await;
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn handle_claim(
    state: &AppState,
    job_id: JobId,
    worker_id: PeerId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claimed = match state.jobs.get_mut(&job_id) {
        None => {
            state
                .gossip
                .broadcast(GossipMessage::ClaimRejected {
                    job_id,
                    reason: "job not found".to_string(),
                })
                .await?;
            return Ok(());
        }
        Some(mut job) => {
            if matches!(job.job_status, JobStatus::Pending) {
                job.job_status = JobStatus::Assigned {
                    worker: worker_id,
                    assigned_at: now,
                };
                true
            } else {
                false
            }
        }
    };

    if claimed {
        state
            .gossip
            .broadcast(GossipMessage::ClaimAccepted { job_id, worker_id })
            .await?;
    } else {
        state
            .gossip
            .broadcast(GossipMessage::ClaimRejected {
                job_id,
                reason: "job already assigned".to_string(),
            })
            .await?;
    }

    Ok(())
}

async fn handle_job_completed(state: &AppState, job_id: JobId, tx_hash: TxHash) {
    tracing::info!(
        job_id = hex::encode(job_id.0),
        tx_hash = hex::encode(tx_hash.0),
        "received JobCompleted via gossip, verifying on-chain"
    );

    // Require on-chain verification before trusting gossip completions
    let contract = match state.contract {
        Some(ref c) => c,
        None => {
            tracing::warn!(
                job_id = hex::encode(job_id.0),
                "no chain client configured, cannot verify gossip completion — ignoring"
            );
            return;
        }
    };

    match contract.is_completed(job_id.0).await {
        Ok(true) => {
            tracing::info!(
                job_id = hex::encode(job_id.0),
                "on-chain verification confirmed, marking completed"
            );
        }
        Ok(false) => {
            tracing::warn!(
                job_id = hex::encode(job_id.0),
                "gossip claimed completion but job is NOT completed on-chain, ignoring"
            );
            return;
        }
        Err(e) => {
            tracing::warn!(
                job_id = hex::encode(job_id.0),
                error = %e,
                "failed to verify completion on-chain, ignoring gossip completion"
            );
            return;
        }
    }

    state.jobs.entry(job_id).and_modify(|job| {
        if !matches!(job.job_status, JobStatus::Completed { .. }) {
            job.job_status = JobStatus::Completed { tx_hash };
            tracing::info!(
                job_id = hex::encode(job_id.0),
                "updated job status to Completed (gossip + chain-verified)"
            );
        }
    });
}
