use worker::{Worker, WorkerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = WorkerConfig {
        gossip_addr: "127.0.0.1:9100".parse()?,
        coordinator_addr: "127.0.0.1:9000".parse()?,
    };

    let worker = Worker::new(config).await?;

    tokio::spawn({
        let g = worker.gossip.clone();
        async move { g.run().await }
    });

    worker.start().await?;
    println!("Worker running with peer_id {:?}", worker.peer_id);

    worker.run().await;

    Ok(())
}
