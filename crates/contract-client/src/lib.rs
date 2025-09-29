use alloy::network::EthereumWallet;
use alloy::primitives::{Address, Bytes, FixedBytes, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use alloy::transports::http::reqwest::Url;
use std::str::FromStr;

sol! {
    #[sol(rpc)]
    contract JobRegistry {
        function submitJob(bytes32 jobId, bytes calldata payload, uint256 deadline) external payable;
        function submitProof(bytes32 jobId, bytes calldata result, bytes calldata seal, bytes32 journalDigest) external;
        function completed(bytes32 jobId) external view returns (bool);

        event JobSubmitted(bytes32 indexed jobId, address indexed owner, uint256 reward, uint256 deadline);
        event JobCompleted(bytes32 indexed jobId, address indexed worker, bytes result);
        event JobExpired(bytes32 indexed jobId);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("transaction failed: {0}")]
    TransactionFailed(String),

    #[error("contract call failed: {0}")]
    CallFailed(String),

    #[error("invalid config: {0}")]
    InvalidConfig(String),
}

pub struct ContractClient {
    rpc_url: Url,
    pub contract_address: Address,
    signer: PrivateKeySigner,
}

impl ContractClient {
    pub fn new(
        rpc_url: &str,
        contract_address: Address,
        private_key: &str,
    ) -> Result<Self, ContractError> {
        let signer = PrivateKeySigner::from_str(private_key)
            .map_err(|e| ContractError::InvalidConfig(e.to_string()))?;
        let url: Url = rpc_url
            .parse()
            .map_err(|e| ContractError::InvalidConfig(format!("{e}")))?;
        Ok(Self {
            rpc_url: url,
            contract_address,
            signer,
        })
    }

    pub async fn submit_job(
        &self,
        job_id: [u8; 32],
        payload: &[u8],
        deadline: u64,
        reward_wei: u128,
    ) -> Result<FixedBytes<32>, ContractError> {
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.clone());
        let contract = JobRegistry::new(self.contract_address, provider);

        let tx = contract
            .submitJob(
                FixedBytes::from(job_id),
                Bytes::copy_from_slice(payload),
                U256::from(deadline),
            )
            .value(U256::from(reward_wei));

        let pending_tx = tx
            .send()
            .await
            .map_err(|e| ContractError::TransactionFailed(e.to_string()))?;

        let receipt = pending_tx
            .get_receipt()
            .await
            .map_err(|e| ContractError::TransactionFailed(e.to_string()))?;

        Ok(receipt.transaction_hash)
    }

    pub async fn submit_proof(
        &self,
        job_id: [u8; 32],
        result: &[u8],
        seal: &[u8],
        journal_digest: [u8; 32],
    ) -> Result<FixedBytes<32>, ContractError> {
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.clone());
        let contract = JobRegistry::new(self.contract_address, provider);

        let tx = contract.submitProof(
            FixedBytes::from(job_id),
            Bytes::copy_from_slice(result),
            Bytes::copy_from_slice(seal),
            FixedBytes::from(journal_digest),
        );

        let pending_tx = tx
            .send()
            .await
            .map_err(|e| ContractError::TransactionFailed(e.to_string()))?;

        let receipt = pending_tx
            .get_receipt()
            .await
            .map_err(|e| ContractError::TransactionFailed(e.to_string()))?;

        Ok(receipt.transaction_hash)
    }

    pub async fn is_completed(&self, job_id: [u8; 32]) -> Result<bool, ContractError> {
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.rpc_url.clone());
        let contract = JobRegistry::new(self.contract_address, provider);

        let res = contract
            .completed(FixedBytes::from(job_id))
            .call()
            .await
            .map_err(|e| ContractError::CallFailed(e.to_string()))?;

        Ok(res)
    }
}
