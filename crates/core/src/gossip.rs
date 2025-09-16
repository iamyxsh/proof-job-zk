use serde::{Deserialize, Serialize};

use crate::{
    enums::GossipMessage,
    ids::{MessageId, PeerId},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GossipEnvelope {
    pub id: MessageId,
    pub origin: PeerId,
    pub ttl: u8,
    pub payload: GossipMessage,
}
