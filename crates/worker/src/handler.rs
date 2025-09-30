use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use proof_core::enums::GossipMessage;
use proof_core::gossip::GossipEnvelope;
use proof_core::ids::{JobId, PeerId};
use proof_core::job::Job;

use crate::{Worker, WorkerStatus};

impl Worker {
    pub(crate) async fn handle_message(
        self: Arc<Self>,
        envelope: GossipEnvelope,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match envelope.payload {
            GossipMessage::JobAvailable(job) => self.handle_job_available(job).await,
            GossipMessage::ClaimAccepted { job_id, worker_id } => {
                self.handle_claim_accepted(job_id, worker_id).await
            }
            GossipMessage::ClaimRejected { job_id, reason } => {
                self.handle_claim_rejected(job_id, reason).await
            }
            _ => Ok(()),
        }
    }

    async fn handle_job_available(
        &self,
        job: Job,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut status = self.status.write().await;

        if !matches!(*status, WorkerStatus::Idle) {
            return Ok(());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if job.deadline <= now {
            return Ok(());
        }

        let job_id = job.id;
        *status = WorkerStatus::Claiming { job };
        drop(status);

        self.gossip
            .broadcast(GossipMessage::ClaimJob {
                job_id,
                worker_id: self.peer_id,
            })
            .await?;

        Ok(())
    }

    async fn handle_claim_accepted(
        self: &Arc<Self>,
        job_id: JobId,
        worker_id: PeerId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut status = self.status.write().await;

        let claiming_match = matches!(
            &*status,
            WorkerStatus::Claiming { job } if job.id == job_id
        );

        if !claiming_match {
            return Ok(());
        }

        if worker_id == self.peer_id {
            let old = std::mem::replace(&mut *status, WorkerStatus::Idle);
            if let WorkerStatus::Claiming { job } = old {
                *status = WorkerStatus::Proving { job: job.clone() };
                tracing::info!(?job_id, "claim accepted, starting proof");
                drop(status);

                // Spawn proving as a separate task so the worker stays responsive
                let worker = Arc::clone(self);
                let payload = job.payload.clone();
                tokio::spawn(async move {
                    Worker::run_proving(worker, job_id, job, payload).await;
                });
            }
        } else {
            *status = WorkerStatus::Idle;
        }

        Ok(())
    }

    async fn handle_claim_rejected(
        &self,
        job_id: JobId,
        _reason: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut status = self.status.write().await;

        let claiming_match = matches!(
            &*status,
            WorkerStatus::Claiming { job } if job.id == job_id
        );

        if claiming_match {
            tracing::info!(?job_id, "claim rejected, returning to idle");
            *status = WorkerStatus::Idle;
        }

        Ok(())
    }

    async fn run_proving(worker: Arc<Worker>, job_id: JobId, job: Job, payload: Vec<u8>) {
        tracing::info!(?job_id, "proving started");

        match prover::prove(job_id, payload).await {
            Ok(bundle) => {
                tracing::info!(
                    ?job_id,
                    receipt_size = bundle.receipt_bytes.len(),
                    "proving complete"
                );

                let mut status = worker.status.write().await;
                if matches!(&*status, WorkerStatus::Proving { job: j } if j.id == job_id) {
                    *status = WorkerStatus::ProofReady { job, bundle };
                    tracing::info!(?job_id, "transitioned to ProofReady");
                }
            }
            Err(e) => {
                tracing::error!(?job_id, error = %e, "proving failed");

                let mut status = worker.status.write().await;
                if matches!(&*status, WorkerStatus::Proving { job: j } if j.id == job_id) {
                    *status = WorkerStatus::Idle;
                }
            }
        }
    }
}
