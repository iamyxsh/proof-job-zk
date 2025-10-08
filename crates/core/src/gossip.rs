use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{
    enums::GossipMessage,
    ids::{MessageId, PeerId},
};

mod sig_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = Vec::deserialize(d)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes for signature"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GossipEnvelope {
    pub id: MessageId,
    pub origin: PeerId,
    pub ttl: u8,
    pub payload: GossipMessage,
    #[serde(with = "sig_bytes")]
    pub signature: [u8; 64],
}

impl GossipEnvelope {
    fn signable_bytes(&self) -> Vec<u8> {
        let tuple = (&self.id, &self.origin, &self.payload);
        bincode::serde::encode_to_vec(&tuple, bincode::config::standard())
            .expect("signable_bytes: bincode encoding failed")
    }

    pub fn sign(&mut self, key: &SigningKey) {
        let msg = self.signable_bytes();
        let sig: Signature = key.sign(&msg);
        self.signature = sig.to_bytes();
    }

    pub fn verify(&self) -> bool {
        let Ok(pubkey) = VerifyingKey::from_bytes(&self.origin.0) else {
            return false;
        };
        let sig = Signature::from_bytes(&self.signature);
        let msg = self.signable_bytes();
        pubkey.verify(&msg, &sig).is_ok()
    }
}
