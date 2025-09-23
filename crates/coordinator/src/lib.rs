pub mod api;
pub mod config;

use std::sync::Arc;

use dashmap::DashMap;
use gossip::gossip::{GossipConfig, GossipNode};
use proof_core::ids::JobId;
use proof_core::job::Job;

use crate::config::CoordinatorConfig;

pub struct AppState {
    pub jobs: DashMap<JobId, Job>,
    pub gossip: GossipNode,
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
    };

    Arc::new(AppState {
        jobs: DashMap::new(),
        gossip,
        config,
    })
}
