use std::sync::Arc;

use coordinator::api;
use coordinator::config::CoordinatorConfig;
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::AppState;
use dashmap::DashMap;
use gossip::gossip::{GossipConfig, GossipNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = CoordinatorConfig {
        http_addr: "127.0.0.1:8080".parse()?,
        gossip_addr: "127.0.0.1:9000".parse()?,
        default_deadline_secs: 300,
    };

    let gossip_config = GossipConfig::new(config.gossip_addr);
    let gossip = GossipNode::new(gossip_config).await?;
    tokio::spawn({
        let g = gossip.clone();
        async move { g.run().await }
    });

    let state = Arc::new(AppState {
        jobs: DashMap::new(),
        gossip,
        config,
    });

    let gossip_rx = state.gossip.subscribe();
    tokio::spawn(run_gossip_handler(state.clone(), gossip_rx));

    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    println!("Coordinator listening on 127.0.0.1:8080");
    axum::serve(listener, app).await?;

    Ok(())
}
