use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::{SinkExt, StreamExt};
use lru::LruCache;
use proof_core::{
    enums::GossipMessage,
    gossip::GossipEnvelope,
    ids::{MessageId, PeerId},
};
use rand::seq::SliceRandom;
use rand::Rng;
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

type ConnectionId = u64;

pub struct GossipConfig {
    pub listen_addr: SocketAddr,
    pub fanout: usize,
    pub max_ttl: u8,
    pub seen_cache_size: usize,
}

impl GossipConfig {
    pub fn new(listen_addr: SocketAddr) -> Self {
        Self {
            listen_addr,
            fanout: 3,
            max_ttl: 3,
            seen_cache_size: 10_000,
        }
    }
}

struct ConnectionHandle {
    outgoing_tx: mpsc::Sender<GossipEnvelope>,
}

struct Inner {
    listener: TcpListener,
    our_peer_id: PeerId,
    config: GossipConfig,
    incoming_tx: mpsc::Sender<(GossipEnvelope, ConnectionId)>,
    incoming_rx: Mutex<Option<mpsc::Receiver<(GossipEnvelope, ConnectionId)>>>,
    subscriber_tx: broadcast::Sender<GossipEnvelope>,
    connections: RwLock<HashMap<ConnectionId, ConnectionHandle>>,
    next_connection_id: AtomicU64,
    seen: Mutex<LruCache<MessageId, ()>>,
}

#[derive(Clone)]
pub struct GossipNode {
    inner: Arc<Inner>,
}

impl GossipNode {
    pub async fn new(config: GossipConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(config.listen_addr).await?;
        let (incoming_tx, incoming_rx) = mpsc::channel(256);
        let (subscriber_tx, _) = broadcast::channel(256);
        let seen_cap = NonZeroUsize::new(config.seen_cache_size).unwrap();
        let our_peer_id = PeerId(rand::thread_rng().gen());

        Ok(Self {
            inner: Arc::new(Inner {
                listener,
                our_peer_id,
                config,
                incoming_tx,
                incoming_rx: Mutex::new(Some(incoming_rx)),
                subscriber_tx,
                connections: RwLock::new(HashMap::new()),
                next_connection_id: AtomicU64::new(0),
                seen: Mutex::new(LruCache::new(seen_cap)),
            }),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.inner.listener.local_addr()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GossipEnvelope> {
        self.inner.subscriber_tx.subscribe()
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut incoming_rx = self
            .inner
            .incoming_rx
            .lock()
            .await
            .take()
            .expect("run() called more than once");

        loop {
            tokio::select! {
                result = self.inner.listener.accept() => {
                    let (stream, _) = result?;
                    self.spawn_connection(stream).await;
                }
                Some((envelope, source)) = incoming_rx.recv() => {
                    self.process_incoming(envelope, source).await;
                }
            }
        }
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stream = TcpStream::connect(addr).await?;
        self.spawn_connection(stream).await;
        Ok(())
    }

    pub async fn broadcast(&self, message: GossipMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let envelope = GossipEnvelope {
            id: self.generate_message_id(&message),
            origin: self.inner.our_peer_id,
            ttl: self.inner.config.max_ttl,
            payload: message,
        };

        self.inner.seen.lock().await.put(envelope.id, ());
        self.send_to_peers(&envelope, None).await
    }

    pub async fn send(&self, envelope: GossipEnvelope) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_to_peers(&envelope, None).await
    }

    async fn send_to_peers(
        &self,
        envelope: &GossipEnvelope,
        exclude: Option<ConnectionId>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let senders: Vec<mpsc::Sender<GossipEnvelope>> = {
            let connections = self.inner.connections.read().await;
            let mut eligible: Vec<&ConnectionHandle> = connections
                .iter()
                .filter(|(id, _)| Some(**id) != exclude)
                .map(|(_, h)| h)
                .collect();

            if eligible.len() > self.inner.config.fanout {
                let mut rng = rand::thread_rng();
                eligible.shuffle(&mut rng);
                eligible.truncate(self.inner.config.fanout);
            }

            eligible.iter().map(|h| h.outgoing_tx.clone()).collect()
        };

        for tx in senders {
            let _ = tx.send(envelope.clone()).await;
        }

        Ok(())
    }

    async fn process_incoming(&self, envelope: GossipEnvelope, source: ConnectionId) {
        {
            let mut seen = self.inner.seen.lock().await;
            if seen.get(&envelope.id).is_some() {
                return;
            }
            seen.put(envelope.id, ());
        }

        let _ = self.inner.subscriber_tx.send(envelope.clone());

        if envelope.ttl > 1 {
            let mut forwarded = envelope;
            forwarded.ttl -= 1;
            let _ = self.send_to_peers(&forwarded, Some(source)).await;
        }
    }

    fn generate_message_id(&self, message: &GossipMessage) -> MessageId {
        let mut hasher = Sha256::new();
        hasher.update(self.inner.our_peer_id.0);
        let msg_bytes = bincode::serde::encode_to_vec(message, bincode::config::standard())
            .expect("failed to encode message for ID generation");
        hasher.update(&msg_bytes);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes();
        hasher.update(nanos);
        let random: [u8; 8] = rand::thread_rng().gen();
        hasher.update(random);
        MessageId(hasher.finalize().into())
    }

    async fn spawn_connection(&self, stream: TcpStream) {
        let conn_id = self.inner.next_connection_id.fetch_add(1, Ordering::Relaxed);
        let incoming_tx = self.inner.incoming_tx.clone();
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<GossipEnvelope>(64);

        self.inner
            .connections
            .write()
            .await
            .insert(conn_id, ConnectionHandle { outgoing_tx });

        tokio::spawn(async move {
            let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
            loop {
                tokio::select! {
                    result = framed.next() => {
                        match result {
                            Some(Ok(bytes)) => {
                                match bincode::serde::decode_from_slice::<GossipEnvelope, _>(&bytes, bincode::config::standard()) {
                                    Ok((envelope, _)) => {
                                        let _ = incoming_tx.send((envelope, conn_id)).await;
                                    }
                                    Err(_) => {
                                        // Malformed frame — drop this connection
                                        break;
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                    Some(envelope) = outgoing_rx.recv() => {
                        let Ok(bytes) = bincode::serde::encode_to_vec(&envelope, bincode::config::standard()) else {
                            break;
                        };
                        if framed.send(bytes.into()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
}
