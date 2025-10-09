pub mod api;
pub mod config;
pub mod gossip_handler;
pub mod persistence;
pub mod rebroadcast;

use std::sync::Arc;

use contract_client::ContractClient;
use dashmap::DashMap;
use gossip::gossip::{GossipConfig, GossipNode};
use proof_core::ids::JobId;
use proof_core::job::Job;

use crate::config::CoordinatorConfig;

pub struct AppState {
    pub jobs: Arc<DashMap<JobId, Job>>,
    pub gossip: GossipNode,
    pub contract: Option<ContractClient>,
    pub config: CoordinatorConfig,
}

pub async fn setup_test_coordinator() -> Arc<AppState> {
    let gossip_config = GossipConfig::new("127.0.0.1:0".parse().unwrap());
    let gossip = GossipNode::new(gossip_config).await.unwrap();
    let gossip_addr = gossip.local_addr().unwrap();

    let config = CoordinatorConfig {
        http_addr: "127.0.0.1:0".parse().unwrap(),
        gossip_addr,
        default_deadline_secs: 300,
        rpc_url: None,
        contract_address: None,
        private_key: None,
    };

    Arc::new(AppState {
        jobs: Arc::new(DashMap::new()),
        gossip,
        contract: None,
        config,
    })
}
