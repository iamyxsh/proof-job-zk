pub mod handler;
pub mod proof;

use std::net::SocketAddr;
use std::sync::Arc;

use alloy_primitives::Address;
use contract_client::ContractClient;
use gossip::gossip::{GossipConfig, GossipNode};
use prover::ProofBundle;
use proof_core::enums::GossipMessage;
use proof_core::ids::{JobId, PeerId, TxHash};
use proof_core::job::Job;
use tokio::sync::RwLock;

use crate::proof::extract_proof_components;

pub struct WorkerConfig {
    pub gossip_addr: SocketAddr,
    pub coordinator_addr: SocketAddr,
    pub rpc_url: String,
    pub private_key: String,
    pub registry_address: Address,
}

#[derive(Debug, Clone)]
pub enum WorkerStatus {
    Idle,
    Claiming { job: Job },
    Proving { job: Job },
    ProofReady { job: Job, bundle: ProofBundle },
    Submitting { job: Job },
}

pub struct Worker {
    pub peer_id: PeerId,
    pub status: RwLock<WorkerStatus>,
    pub gossip: GossipNode,
    pub chain: ContractClient,
    pub config: WorkerConfig,
}

impl Worker {
    pub async fn new(
        config: WorkerConfig,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let peer_id = PeerId(rand::random());
        let gossip_config = GossipConfig::new(config.gossip_addr);
        let gossip = GossipNode::new(gossip_config).await?;

        let chain = ContractClient::new(
            &config.rpc_url,
            config.registry_address,
            &config.private_key,
        )?;

        tracing::info!(
            peer_id = hex::encode(peer_id.0),
            registry = %config.registry_address,
            "worker initialized with chain client"
        );

        Ok(Arc::new(Self {
            peer_id,
            status: RwLock::new(WorkerStatus::Idle),
            gossip,
            chain,
            config,
        }))
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.gossip.connect(self.config.coordinator_addr).await?;
        Ok(())
    }

    pub async fn run(self: Arc<Self>) {
        let mut rx = self.gossip.subscribe();

        loop {
            match rx.recv().await {
                Ok(envelope) => {
                    if let Err(e) = self.clone().handle_message(envelope).await {
                        eprintln!("Error handling message: {}", e);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("Worker lagged, missed {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    }

    pub(crate) async fn submit_proof_onchain(
        self: &Arc<Self>,
        job: Job,
        bundle: ProofBundle,
    ) {
        let job_id = job.id;

        let components = match extract_proof_components(&bundle) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(?job_id, error = %e, "failed to extract proof components");
                self.transition_to_idle(job_id).await;
                return;
            }
        };

        tracing::info!(
            ?job_id,
            journal_len = components.journal.len(),
            seal_len = components.seal.len(),
            "submitting proof on-chain"
        );

        let tx_hash = match self
            .chain
            .submit_proof(
                job_id.0,
                &components.journal,
                &components.seal,
            )
            .await
        {
            Ok(hash) => hash,
            Err(e) => {
                tracing::error!(?job_id, error = %e, "on-chain proof submission failed");
                self.transition_to_idle(job_id).await;
                return;
            }
        };

        tracing::info!(
            ?job_id,
            tx_hash = %tx_hash,
            "proof submitted on-chain successfully"
        );

        let tx_hash_core = TxHash(tx_hash.0);
        if let Err(e) = self
            .gossip
            .broadcast(GossipMessage::JobCompleted {
                job_id,
                tx_hash: tx_hash_core,
            })
            .await
        {
            tracing::warn!(?job_id, error = %e, "failed to broadcast JobCompleted");
        }

        self.transition_to_idle(job_id).await;
        tracing::info!(?job_id, "job complete, returning to idle");
    }

    pub(crate) async fn transition_to_idle(&self, expected_job_id: JobId) {
        let mut status = self.status.write().await;
        match &*status {
            WorkerStatus::Submitting { job } if job.id == expected_job_id => {
                *status = WorkerStatus::Idle;
            }
            WorkerStatus::Proving { job } if job.id == expected_job_id => {
                *status = WorkerStatus::Idle;
            }
            _ => {
                tracing::warn!(
                    ?expected_job_id,
                    "unexpected state during transition to idle"
                );
            }
        }
    }
}
