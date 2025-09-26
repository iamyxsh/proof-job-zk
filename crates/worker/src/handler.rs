use std::time::{SystemTime, UNIX_EPOCH};

use proof_core::enums::GossipMessage;
use proof_core::gossip::GossipEnvelope;
use proof_core::ids::{JobId, PeerId};
use proof_core::job::Job;

use crate::{Worker, WorkerStatus};

impl Worker {
    pub(crate) async fn handle_message(
        &self,
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
        &self,
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
                *status = WorkerStatus::Assigned { job };
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
            *status = WorkerStatus::Idle;
        }

        Ok(())
    }
}
