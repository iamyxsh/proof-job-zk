use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use contract_client::JobRegistry;
use dashmap::DashMap;
use proof_core::enums::JobStatus;
use proof_core::ids::{JobId, TxHash};
use proof_core::job::Job;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

#[derive(Debug, thiserror::Error)]
pub enum IndexerError {
    #[error("provider error: {0}")]
    Provider(String),

    #[error("event decode error: {0}")]
    EventDecode(String),
}

pub struct IndexerConfig {
    pub rpc_url: String,
    pub registry_address: Address,
    pub poll_interval: Duration,
    pub start_block: Option<u64>,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:8545".to_string(),
            registry_address: Address::ZERO,
            poll_interval: Duration::from_secs(2),
            start_block: None,
        }
    }
}

pub struct Indexer<P: Provider> {
    provider: P,
    registry_address: Address,
    jobs: Arc<DashMap<JobId, Job>>,
    poll_interval: Duration,
    last_block: u64,
}

impl<P: Provider + Clone> Indexer<P> {
    pub async fn new(
        provider: P,
        config: IndexerConfig,
        jobs: Arc<DashMap<JobId, Job>>,
    ) -> Result<Self, IndexerError> {
        let last_block = match config.start_block {
            Some(block) => block,
            None => provider
                .get_block_number()
                .await
                .map_err(|e| IndexerError::Provider(e.to_string()))?,
        };

        tracing::info!(
            registry = %config.registry_address,
            start_block = last_block,
            "indexer initialized"
        );

        Ok(Self {
            provider,
            registry_address: config.registry_address,
            jobs,
            poll_interval: config.poll_interval,
            last_block,
        })
    }

    pub async fn run(mut self) {
        let mut poll_timer = interval(self.poll_interval);

        loop {
            poll_timer.tick().await;

            if let Err(e) = self.poll_events().await {
                tracing::warn!(error = %e, "error polling events");
            }
        }
    }

    async fn poll_events(&mut self) -> Result<(), IndexerError> {
        let current_block = self
            .provider
            .get_block_number()
            .await
            .map_err(|e| IndexerError::Provider(e.to_string()))?;

        if current_block <= self.last_block {
            return Ok(());
        }

        let from_block = self.last_block + 1;
        let to_block = current_block;

        tracing::debug!(from = from_block, to = to_block, "polling blocks");

        self.process_job_completed_events(from_block, to_block)
            .await?;
        self.process_job_expired_events(from_block, to_block)
            .await?;

        self.last_block = current_block;

        Ok(())
    }

    async fn process_job_completed_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<(), IndexerError> {
        let contract = JobRegistry::new(self.registry_address, &self.provider);

        let filter = contract
            .JobCompleted_filter()
            .from_block(from_block)
            .to_block(to_block);

        let logs = filter
            .query()
            .await
            .map_err(|e| IndexerError::Provider(e.to_string()))?;

        for (event, log) in logs {
            let job_id = JobId(event.jobId.0);
            let tx_hash = TxHash(log.transaction_hash.unwrap_or_default().0);

            tracing::info!(
                job_id = hex::encode(job_id.0),
                worker = %event.worker,
                tx_hash = hex::encode(tx_hash.0),
                "JobCompleted event"
            );

            self.handle_job_completed(job_id, tx_hash);
        }

        Ok(())
    }

    async fn process_job_expired_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<(), IndexerError> {
        let contract = JobRegistry::new(self.registry_address, &self.provider);

        let filter = contract
            .JobExpired_filter()
            .from_block(from_block)
            .to_block(to_block);

        let logs = filter
            .query()
            .await
            .map_err(|e| IndexerError::Provider(e.to_string()))?;

        for (event, _log) in logs {
            let job_id = JobId(event.jobId.0);

            tracing::info!(job_id = hex::encode(job_id.0), "JobExpired event");

            self.handle_job_expired(job_id);
        }

        Ok(())
    }

    fn handle_job_completed(&self, job_id: JobId, tx_hash: TxHash) {
        if !self.jobs.contains_key(&job_id) {
            tracing::warn!(
                job_id = hex::encode(job_id.0),
                "received JobCompleted for unknown job"
            );
            return;
        }

        self.jobs.entry(job_id).and_modify(|job| {
            if !matches!(job.job_status, JobStatus::Completed { .. }) {
                tracing::info!(
                    job_id = hex::encode(job_id.0),
                    "updating job status to Completed (from indexer)"
                );
                job.job_status = JobStatus::Completed { tx_hash };
            } else {
                tracing::debug!(
                    job_id = hex::encode(job_id.0),
                    "job already completed, skipping"
                );
            }
        });
    }

    fn handle_job_expired(&self, job_id: JobId) {
        self.jobs.entry(job_id).and_modify(|job| {
            if !matches!(
                job.job_status,
                JobStatus::Completed { .. } | JobStatus::Expired
            ) {
                tracing::info!(
                    job_id = hex::encode(job_id.0),
                    "updating job status to Expired (from indexer)"
                );
                job.job_status = JobStatus::Expired;
            }
        });
    }
}

pub async fn create_indexer(
    config: IndexerConfig,
    jobs: Arc<DashMap<JobId, Job>>,
) -> Result<Indexer<impl Provider + Clone>, IndexerError> {
    let url: alloy::transports::http::reqwest::Url = config
        .rpc_url
        .parse()
        .map_err(|e| IndexerError::Provider(format!("invalid rpc url: {e}")))?;

    let provider = ProviderBuilder::new().connect_http(url);

    Indexer::new(provider, config, jobs).await
}
