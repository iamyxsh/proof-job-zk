use std::net::SocketAddr;

pub struct CoordinatorConfig {
    pub http_addr: SocketAddr,
    pub gossip_addr: SocketAddr,
    pub default_deadline_secs: u64,
    pub rpc_url: Option<String>,
    pub contract_address: Option<String>,
    pub private_key: Option<String>,
}
