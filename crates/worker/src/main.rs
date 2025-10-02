use alloy_primitives::Address;
use worker::{Worker, WorkerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter("worker=debug,prover=info,gossip=info")
        .init();

    let registry_address: Address = std::env::var("REGISTRY_ADDRESS")
        .expect("REGISTRY_ADDRESS required")
        .parse()
        .expect("invalid REGISTRY_ADDRESS");

    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());

    let private_key = std::env::var("WORKER_PRIVATE_KEY").unwrap_or_else(|_| {
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".to_string()
    });

    let gossip_addr = std::env::var("GOSSIP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9100".to_string())
        .parse()?;

    let coordinator_addr = std::env::var("COORDINATOR_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9000".to_string())
        .parse()?;

    let config = WorkerConfig {
        gossip_addr,
        coordinator_addr,
        rpc_url,
        private_key,
        registry_address,
    };

    let worker = Worker::new(config).await?;

    tokio::spawn({
        let g = worker.gossip.clone();
        async move { g.run().await }
    });

    worker.start().await?;
    tracing::info!(peer_id = hex::encode(worker.peer_id.0), "worker running");

    worker.run().await;

    Ok(())
}
