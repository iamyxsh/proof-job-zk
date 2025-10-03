use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Address;
use contract_client::ContractClient;
use coordinator::api;
use coordinator::config::CoordinatorConfig;
use coordinator::gossip_handler::run_gossip_handler;
use coordinator::AppState;
use dashmap::DashMap;
use gossip::gossip::{GossipConfig, GossipNode};
use indexer::{create_indexer, IndexerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rpc_url = std::env::var("RPC_URL").ok();
    let contract_address = std::env::var("CONTRACT_ADDRESS").ok();
    let private_key = std::env::var("PRIVATE_KEY").ok();

    let config = CoordinatorConfig {
        http_addr: std::env::var("HTTP_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()?,
        gossip_addr: std::env::var("GOSSIP_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:9000".to_string())
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
            println!("Contract client initialized at {addr}");
            Some(client)
        }
        _ => {
            println!("No contract config provided, running without on-chain integration");
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

    let jobs = Arc::new(DashMap::new());

    let state = Arc::new(AppState {
        jobs: jobs.clone(),
        gossip,
        contract,
        config,
    });

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
        println!("Indexer started, polling {addr}");
    }

    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    println!("Coordinator listening on {http_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
