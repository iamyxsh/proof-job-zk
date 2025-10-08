use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use contract_client::ContractClient;
use coordinator::api;
use coordinator::config::CoordinatorConfig;
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::persistence;
use coordinator::AppState;
use gossip::gossip::{GossipConfig, GossipNode};
use indexer::{create_indexer, IndexerConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("coordinator=info,indexer=info,gossip=info")),
        )
        .init();

    let rpc_url = std::env::var("RPC_URL").ok();
    let contract_address = std::env::var("CONTRACT_ADDRESS")
        .or_else(|_| std::env::var("REGISTRY_ADDRESS"))
        .ok();
    let private_key = std::env::var("PRIVATE_KEY").ok();

    let http_port = std::env::var("HTTP_PORT").unwrap_or_else(|_| "8080".to_string());
    let gossip_port = std::env::var("GOSSIP_PORT").unwrap_or_else(|_| "9000".to_string());

    let config = CoordinatorConfig {
        http_addr: std::env::var("HTTP_ADDR")
            .unwrap_or_else(|_| format!("127.0.0.1:{}", http_port))
            .parse()?,
        gossip_addr: std::env::var("GOSSIP_ADDR")
            .unwrap_or_else(|_| format!("127.0.0.1:{}", gossip_port))
            .parse()?,
        default_deadline_secs: 300,
        rpc_url: rpc_url.clone(),
        contract_address: contract_address.clone(),
        private_key: private_key.clone(),
    };

    let contract = match (&rpc_url, &contract_address, &private_key) {
        (Some(rpc), Some(addr), Some(pk)) => {
            let parsed_addr = addr.parse().map_err(|e| format!("invalid CONTRACT_ADDRESS: {e}"))?;
            let client = ContractClient::new(rpc, parsed_addr, pk)?;
            tracing::info!(address = %addr, "contract client initialized");
            Some(client)
        }
        _ => {
            tracing::warn!("no contract config provided, running without on-chain integration");
            None
        }
    };

    let gossip_config = GossipConfig::new(config.gossip_addr);
    let gossip = GossipNode::new(gossip_config).await?;
    tokio::spawn({
        let g = gossip.clone();
        async move { g.run().await }
    });

    let http_addr = config.http_addr;

    let snapshot_path = persistence::snapshot_path();
    let jobs = persistence::load_jobs(&snapshot_path);

    let state = Arc::new(AppState {
        jobs: jobs.clone(),
        gossip,
        contract,
        config,
    });

    persistence::spawn_snapshot_task(jobs.clone(), snapshot_path, Duration::from_secs(5));

    let gossip_rx = state.gossip.subscribe();
    tokio::spawn(run_gossip_handler(state.clone(), gossip_rx));

    if let (Some(rpc), Some(addr)) = (&state.config.rpc_url, &state.config.contract_address) {
        let registry_address: Address = addr
            .parse()
            .expect("invalid CONTRACT_ADDRESS for indexer");
        let indexer_config = IndexerConfig {
            rpc_url: rpc.clone(),
            registry_address,
            poll_interval: Duration::from_secs(2),
            start_block: None,
        };
        let indexer = create_indexer(indexer_config, jobs.clone())
            .await
            .expect("failed to start indexer");
        tokio::spawn(indexer.run());
        tracing::info!(address = %addr, "indexer started");
    }

    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!(%http_addr, "coordinator listening");
    axum::serve(listener, app).await?;

    Ok(())
}
