pub mod handler;

use std::net::SocketAddr;
use std::sync::Arc;

use gossip::gossip::{GossipConfig, GossipNode};
use proof_core::ids::PeerId;
use proof_core::job::Job;
use tokio::sync::RwLock;

pub struct WorkerConfig {
    pub gossip_addr: SocketAddr,
    pub coordinator_addr: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    Idle,
    Claiming { job: Job },
    Assigned { job: Job },
}

pub struct Worker {
    pub peer_id: PeerId,
    pub status: RwLock<WorkerStatus>,
    pub gossip: GossipNode,
    pub config: WorkerConfig,
}

impl Worker {
    pub async fn new(
        config: WorkerConfig,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let peer_id = PeerId(rand::random());
        let gossip_config = GossipConfig::new(config.gossip_addr);
        let gossip = GossipNode::new(gossip_config).await?;

        Ok(Arc::new(Self {
            peer_id,
            status: RwLock::new(WorkerStatus::Idle),
            gossip,
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
                    if let Err(e) = self.handle_message(envelope).await {
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
}
