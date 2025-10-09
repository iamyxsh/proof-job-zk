use std::sync::Arc;
use std::time::Duration;

use proof_core::enums::{GossipMessage, JobStatus};

use crate::AppState;

pub async fn run_pending_rebroadcast(state: Arc<AppState>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip immediate first tick

    loop {
        ticker.tick().await;

        let pending_jobs: Vec<_> = state
            .jobs
            .iter()
            .filter(|entry| matches!(entry.value().job_status, JobStatus::Pending))
            .map(|entry| entry.value().clone())
            .collect();

        for job in pending_jobs {
            tracing::debug!(job_id = hex::encode(job.id.0), "re-broadcasting pending job");
            if let Err(e) = state
                .gossip
                .broadcast(GossipMessage::JobAvailable(job))
                .await
            {
                tracing::warn!(error = %e, "failed to re-broadcast pending job");
            }
        }
    }
}
