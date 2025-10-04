use proof_core::{ids::JobId, job::JobOutput};
use risc0_zkvm::{default_prover, ExecutorEnv};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

pub use methods::{GUEST_ELF, GUEST_ID};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBundle {
    pub job_id: JobId,
    pub output: JobOutput,
    pub receipt_bytes: Vec<u8>,
    pub image_id: [u32; 8],
}

#[derive(Debug, thiserror::Error)]
pub enum ProverError {
    #[error("failed to build executor env: {0}")]
    EnvBuildFailed(String),

    #[error("proving failed: {0}")]
    ProvingFailed(String),

    #[error("failed to decode journal: {0}")]
    JournalDecodeFailed(String),

    #[error("serialization failed: {0}")]
    SerializationFailed(String),

    #[error("task join failed: {0}")]
    JoinFailed(#[from] tokio::task::JoinError),
}

pub async fn prove(job_id: JobId, data: Vec<u8>) -> Result<ProofBundle, ProverError> {
    spawn_blocking(move || prove_sync(job_id, data)).await?
}

fn prove_sync(job_id: JobId, data: Vec<u8>) -> Result<ProofBundle, ProverError> {
    tracing::info!(?job_id, data_len = data.len(), "starting proof generation");

    let input = proof_core::job::JobInput {
        job_id,
        data,
    };

    let env = ExecutorEnv::builder()
        .write(&input)
        .map_err(|e| ProverError::EnvBuildFailed(e.to_string()))?
        .build()
        .map_err(|e| ProverError::EnvBuildFailed(e.to_string()))?;

    let prover = default_prover();

    tracing::debug!(?job_id, "calling prover.prove()");
    let prove_info = prover
        .prove(env, GUEST_ELF)
        .map_err(|e| ProverError::ProvingFailed(e.to_string()))?;

    let receipt = prove_info.receipt;

    tracing::debug!(?job_id, "proof generated, decoding journal");

    // Journal layout (raw bytes committed by guest):
    //   [0..32)   job_id
    //   [32..64)  payload_hash  (sha256 of input data)
    //   [64..)    result
    let journal = &receipt.journal.bytes;
    if journal.len() < 64 {
        return Err(ProverError::JournalDecodeFailed(
            format!("journal too short: {} bytes (expected >= 64)", journal.len()),
        ));
    }

    let decoded_job_id = JobId(journal[0..32].try_into().unwrap());
    let payload_hash: [u8; 32] = journal[32..64].try_into().unwrap();
    let result = journal[64..].to_vec();

    let output = JobOutput {
        job_id: decoded_job_id,
        payload_hash,
        result,
    };

    let receipt_bytes = bincode::serde::encode_to_vec(&receipt, bincode::config::standard())
        .map_err(|e| ProverError::SerializationFailed(e.to_string()))?;

    tracing::info!(
        ?job_id,
        receipt_size = receipt_bytes.len(),
        "proof generation complete"
    );

    Ok(ProofBundle {
        job_id,
        output,
        receipt_bytes,
        image_id: GUEST_ID,
    })
}

pub fn verify(bundle: &ProofBundle) -> Result<(), ProverError> {
    let (receipt, _): (risc0_zkvm::Receipt, _) =
        bincode::serde::decode_from_slice(&bundle.receipt_bytes, bincode::config::standard())
            .map_err(|e| ProverError::SerializationFailed(e.to_string()))?;

    receipt
        .verify(bundle.image_id)
        .map_err(|e| ProverError::ProvingFailed(format!("verification failed: {e}")))?;

    Ok(())
}

pub fn guest_image_id() -> [u32; 8] {
    GUEST_ID
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn prove_returns_valid_bundle() {
        let _ = tracing_subscriber::fmt::try_init();

        let job_id = JobId([0xaa; 32]);
        let data = 10u64.to_le_bytes().to_vec();

        let bundle = prove(job_id, data.clone()).await.expect("proving failed");

        assert_eq!(bundle.job_id, job_id);
        assert_eq!(bundle.output.job_id, job_id);

        let expected_payload_hash: [u8; 32] = Sha256::digest(&data).into();
        assert_eq!(bundle.output.payload_hash, expected_payload_hash);

        let result = u64::from_le_bytes(
            bundle.output.result[..8].try_into().unwrap(),
        );
        assert_eq!(result, 55);
        assert_eq!(bundle.image_id, GUEST_ID);
        assert!(!bundle.receipt_bytes.is_empty());
    }

    #[tokio::test]
    async fn proof_verifies_locally() {
        let _ = tracing_subscriber::fmt::try_init();

        let job_id = JobId([0xbb; 32]);
        let data = 5u64.to_le_bytes().to_vec();

        let bundle = prove(job_id, data).await.expect("proving failed");

        verify(&bundle).expect("verification failed");
    }

    #[tokio::test]
    async fn different_inputs_different_outputs() {
        let _ = tracing_subscriber::fmt::try_init();

        let job_id = JobId([0xcc; 32]);

        let bundle_5 = prove(job_id, 5u64.to_le_bytes().to_vec())
            .await
            .expect("proving fib(5) failed");

        let bundle_10 = prove(job_id, 10u64.to_le_bytes().to_vec())
            .await
            .expect("proving fib(10) failed");

        let result_5 = u64::from_le_bytes(
            bundle_5.output.result[..8].try_into().unwrap(),
        );
        let result_10 = u64::from_le_bytes(
            bundle_10.output.result[..8].try_into().unwrap(),
        );

        assert_eq!(result_5, 5);
        assert_eq!(result_10, 55);
        assert_ne!(result_5, result_10);
    }

    #[test]
    fn guest_image_id_is_valid() {
        let id = guest_image_id();
        assert_ne!(id, [0u32; 8]);
        assert_eq!(id, GUEST_ID);
    }
}
