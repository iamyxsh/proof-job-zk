use std::net::SocketAddr;

pub struct CoordinatorConfig {
    pub http_addr: SocketAddr,
    pub gossip_addr: SocketAddr,
    pub default_deadline_secs: u64,
}
