use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use proof_core::gossip::GossipEnvelope;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

struct Inner {
    listener: TcpListener,
    /// Outbound: fan-out envelopes to all connection writers.
    broadcast_tx: broadcast::Sender<GossipEnvelope>,
    /// Inbound: fan-out received envelopes to all subscribers.
    incoming_tx: broadcast::Sender<GossipEnvelope>,
}

#[derive(Clone)]
pub struct GossipNode {
    inner: Arc<Inner>,
}

impl GossipNode {
    pub async fn new(addr: SocketAddr) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        let (broadcast_tx, _) = broadcast::channel::<GossipEnvelope>(256);
        let (incoming_tx, _) = broadcast::channel::<GossipEnvelope>(256);
        Ok(Self {
            inner: Arc::new(Inner {
                listener,
                broadcast_tx,
                incoming_tx,
            }),
        })
    }

    /// Returns a broadcast receiver that yields every inbound envelope.
    pub fn subscribe(&self) -> broadcast::Receiver<GossipEnvelope> {
        self.inner.incoming_tx.subscribe()
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        loop {
            let (stream, _addr) = self.inner.listener.accept().await?;
            self.spawn_connection(stream);
        }
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stream = TcpStream::connect(addr).await?;
        self.spawn_connection(stream);
        Ok(())
    }

    pub async fn send(&self, envelope: GossipEnvelope) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.broadcast_tx.send(envelope)?;
        Ok(())
    }

    fn spawn_connection(&self, stream: TcpStream) {
        let incoming_tx = self.inner.incoming_tx.clone();
        let mut broadcast_rx = self.inner.broadcast_tx.subscribe();

        tokio::spawn(async move {
            let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
            loop {
                tokio::select! {
                    result = framed.next() => {
                        match result {
                            Some(Ok(bytes)) => {
                                let (envelope, _): (GossipEnvelope, _) =
                                    bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
                                        .expect("failed to decode envelope");
                                let _ = incoming_tx.send(envelope);
                            }
                            _ => break,
                        }
                    }
                    Ok(envelope) = broadcast_rx.recv() => {
                        let bytes = bincode::serde::encode_to_vec(&envelope, bincode::config::standard())
                            .expect("failed to encode envelope");
                        if framed.send(bytes.into()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
}
