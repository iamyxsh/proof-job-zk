use prover::ProofBundle;

#[derive(Debug, thiserror::Error)]
pub enum ProofExtractionError {
    #[error("failed to deserialize receipt: {0}")]
    DeserializeFailed(String),
}

pub struct ProofComponents {
    pub journal: Vec<u8>,
    pub seal: Vec<u8>,
}

pub fn extract_proof_components(
    bundle: &ProofBundle,
) -> Result<ProofComponents, ProofExtractionError> {
    let (receipt, _): (risc0_zkvm::Receipt, _) =
        bincode::serde::decode_from_slice(&bundle.receipt_bytes, bincode::config::standard())
            .map_err(|e| ProofExtractionError::DeserializeFailed(e.to_string()))?;

    let seal = match receipt.inner.groth16() {
        Ok(groth16_receipt) => groth16_receipt.seal.clone(),
        Err(_) => {
            tracing::warn!("no Groth16 seal found, using empty seal (dev mode?)");
            vec![]
        }
    };

    let journal = receipt.journal.bytes.clone();

    Ok(ProofComponents {
        journal,
        seal,
    })
}
