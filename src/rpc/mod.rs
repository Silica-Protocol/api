use std::time::Duration;

use anyhow::{Context, Result};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::rpc_params;
use serde::Deserialize;
use silica::contracts::DeploymentManifest;
use silica::privacy::{SpendPublicKey, ViewPublicKey};
use silica::types::Block;

use crate::models::privacy::{
    StealthAddressRequestPayload, StealthAddressResponsePayload, StealthTransferRequestPayload,
    StealthTransferResponsePayload,
};

#[derive(Clone)]
pub struct RpcClient {
    inner: HttpClient,
    timeout: Duration,
}

impl RpcClient {
    pub fn new(endpoint: &str, timeout: Duration) -> Result<Self> {
        assert!(!endpoint.is_empty(), "RPC endpoint must be provided");
        assert!(
            timeout >= Duration::from_millis(100),
            "Timeout below 100ms is unsafe"
        );

        let client = HttpClientBuilder::default()
            .request_timeout(timeout)
            .build(endpoint)
            .with_context(|| format!("Failed to build RPC client for {endpoint}"))?;

        Ok(Self {
            inner: client,
            timeout,
        })
    }

    pub fn timeout(&self) -> Duration {
        assert!(
            self.timeout >= Duration::from_millis(100),
            "Timeout invariant broken"
        );
        assert!(
            self.timeout <= Duration::from_secs(60),
            "Timeout exceeds maximum bound"
        );
        self.timeout
    }

    pub async fn fetch_latest_block_number(&self) -> Result<u64> {
        let response: BlockNumberResponse = self
            .inner
            .request("eth_blockNumber", rpc_params![])
            .await
            .context("RPC call eth_blockNumber failed")?;
        assert!(
            response.block_number <= i64::MAX as u64,
            "Block height exceeds storage bounds"
        );
        assert!(
            response.block_number < 1_000_000_000_000,
            "Block height sanity check failed"
        );
        Ok(response.block_number)
    }

    pub async fn fetch_blocks(&self) -> Result<Vec<Block>> {
        let response: BlocksResponse = self
            .inner
            .request("get_blocks", rpc_params![])
            .await
            .context("RPC call get_blocks failed")?;
        assert!(
            response.blocks.len() <= 10_000,
            "Block batch exceeded defensive limit"
        );
        assert!(
            response.blocks.iter().all(|b| !b.block_hash.is_empty()),
            "RPC returned block with empty hash"
        );
        Ok(response.blocks)
    }

    pub async fn fetch_identity_registry(
        &self,
        from_block: u64,
        limit: u64,
    ) -> Result<IdentityRegistryResponse> {
        assert!(limit > 0, "Identity registry limit must be positive");
        assert!(
            limit <= 1024,
            "Identity registry limit exceeds defensive bound"
        );
        let response: IdentityRegistryResponse = self
            .inner
            .request("identity_registryUpdates", rpc_params![from_block, limit])
            .await
            .context("RPC call identity_registryUpdates failed")?;
        assert!(
            response.latest_block >= from_block,
            "Identity registry latest block regressed"
        );
        assert!(
            response.updates.len() <= limit as usize,
            "Identity registry response exceeded requested limit"
        );
        Ok(response)
    }

    pub async fn generate_stealth_address(
        &self,
        request: &StealthAddressRequestPayload,
    ) -> Result<StealthAddressResponsePayload> {
        let mut params = ObjectParams::new();
        if let Some(seed) = &request.seed_hex {
            params
                .insert("seed_hex", seed)
                .context("Failed to encode seed_hex parameter")?;
        }
        params
            .insert("include_secrets", request.include_secrets)
            .context("Failed to encode include_secrets parameter")?;

        let response: StealthAddressResponsePayload = self
            .inner
            .request("privacy_generateStealthAddress", params)
            .await
            .context("RPC call privacy_generateStealthAddress failed")?;

        assert!(
            !response.address.is_empty(),
            "RPC returned empty stealth address"
        );
        assert_eq!(
            response.view_key.len(),
            64,
            "View key hex encoding must be 32 bytes",
        );

        Ok(response)
    }

    pub async fn submit_stealth_transfer(
        &self,
        request: &StealthTransferRequestPayload,
        recipient_view_key: &ViewPublicKey,
        recipient_spend_key: &SpendPublicKey,
    ) -> Result<StealthTransferResponsePayload> {
        let mut params = ObjectParams::new();
        params
            .insert("sender_keys", &request.sender_keys)
            .context("Failed to encode sender_keys parameter")?;
        params
            .insert("recipient_view_key", recipient_view_key)
            .context("Failed to encode recipient_view_key parameter")?;
        params
            .insert("recipient_spend_key", recipient_spend_key)
            .context("Failed to encode recipient_spend_key parameter")?;
        params
            .insert("amount", request.amount)
            .context("Failed to encode amount parameter")?;
        params
            .insert("fee", request.fee)
            .context("Failed to encode fee parameter")?;
        params
            .insert("nonce", request.nonce)
            .context("Failed to encode nonce parameter")?;
        params
            .insert("privacy_level", request.privacy_level.as_str())
            .context("Failed to encode privacy_level parameter")?;
        if let Some(memo) = &request.memo {
            params
                .insert("memo", memo)
                .context("Failed to encode memo parameter")?;
        }

        let response: StealthTransferResponsePayload = self
            .inner
            .request("privacy_submitStealthTransfer", params)
            .await
            .context("RPC call privacy_submitStealthTransfer failed")?;

        assert!(
            !response.tx_hash.is_empty(),
            "RPC returned empty transaction hash",
        );

        Ok(response)
    }

    #[allow(dead_code)]
    pub async fn deploy_contract(
        &self,
        request: &ContractDeploymentRequest,
    ) -> Result<ContractDeploymentResponse> {
        assert!(
            !request.deployer.is_empty(),
            "Deployer address must be provided",
        );
        assert!(
            !request.wasm_hex.is_empty(),
            "WASM payload must be provided",
        );
        assert!(!request.timestamp.is_empty(), "Timestamp must be provided",);
        assert!(
            !request.signature_hex.is_empty(),
            "Signature must be provided",
        );

        let mut params = ObjectParams::new();
        params
            .insert("deployer", &request.deployer)
            .context("Failed to encode deployer parameter")?;
        params
            .insert("wasm_hex", &request.wasm_hex)
            .context("Failed to encode wasm_hex parameter")?;
        params
            .insert("manifest", &request.manifest)
            .context("Failed to encode manifest parameter")?;
        params
            .insert("fee", request.fee)
            .context("Failed to encode fee parameter")?;
        params
            .insert("nonce", request.nonce)
            .context("Failed to encode nonce parameter")?;
        params
            .insert("timestamp", &request.timestamp)
            .context("Failed to encode timestamp parameter")?;
        params
            .insert("signature", &request.signature_hex)
            .context("Failed to encode signature parameter")?;
        if let Some(tx_id) = &request.tx_id {
            params
                .insert("tx_id", tx_id)
                .context("Failed to encode tx_id parameter")?;
        }

        let response: ContractDeploymentResponse = self
            .inner
            .request("contracts_deploy", params)
            .await
            .context("RPC call contracts_deploy failed")?;

        assert!(
            !response.tx_id.is_empty(),
            "RPC returned empty deployment tx_id",
        );
        assert!(
            !response.status.is_empty(),
            "RPC returned empty deployment status",
        );
        assert!(
            !response.contract_address.is_empty(),
            "RPC returned empty contract address",
        );

        Ok(response)
    }

    /// Cast a vote on a governance proposal via RPC
    pub async fn governance_cast_vote(
        &self,
        proposal_id: &str,
        voter: &str,
        approve: bool,
    ) -> Result<GovernanceVoteResponse> {
        assert!(!proposal_id.is_empty(), "Proposal ID must not be empty");
        assert!(!voter.is_empty(), "Voter address must be provided");

        let support = if approve { 1i32 } else { 0i32 };
        let response: GovernanceVoteResponse = self
            .inner
            .request(
                "governance_castVote",
                rpc_params![proposal_id, voter, support],
            )
            .await
            .context("RPC call governance_castVote failed")?;

        Ok(response)
    }

    /// Delegate voting stake to a validator via RPC
    pub async fn governance_delegate_stake(
        &self,
        delegator: &str,
        validator: &str,
        amount: u64,
    ) -> Result<GovernanceDelegateResponse> {
        assert!(!delegator.is_empty(), "Delegator address must be provided");
        assert!(!validator.is_empty(), "Validator address must be provided");
        assert!(amount > 0, "Delegation amount must be positive");

        let response: GovernanceDelegateResponse = self
            .inner
            .request(
                "governance_delegateStake",
                rpc_params![delegator, validator, amount],
            )
            .await
            .context("RPC call governance_delegateStake failed")?;

        Ok(response)
    }
}

#[derive(Debug, Deserialize)]
pub struct GovernanceVoteResponse {
    pub status: String,
    pub votes_for: u64,
    pub votes_against: u64,
    pub voter: String,
    pub vote_weight: u64,
    pub approve: bool,
    pub finalized: bool,
}

#[derive(Debug, Deserialize)]
pub struct GovernanceDelegateResponse {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub delegation: DelegationRpcRecord,
}

#[derive(Debug, Deserialize)]
pub struct DelegationRpcRecord {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub delegated_at: String,
}

#[derive(Debug, Deserialize)]
struct BlockNumberResponse {
    pub block_number: u64,
}

#[derive(Debug, Deserialize)]
struct BlocksResponse {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Deserialize)]
pub struct IdentityRegistryResponse {
    pub latest_block: u64,
    #[serde(default)]
    pub updates: Vec<IdentityRecord>,
}

#[derive(Debug, Deserialize)]
pub struct IdentityRecord {
    pub identity_id: String,
    pub display_name: Option<String>,
    pub avatar_hash: Option<String>,
    pub bio: Option<String>,
    pub stats_visibility: String,
    #[serde(default)]
    pub wallet_links: Vec<WalletLinkRecord>,
    pub created_at: u64,
    pub updated_at: u64,
    pub updated_at_block: u64,
    #[serde(default)]
    pub profile_version: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct WalletLinkRecord {
    pub wallet_address: String,
    pub link_type: String,
    pub proof_signature: String,
    pub created_at: u64,
    pub verified_at: Option<u64>,
    pub updated_at_block: u64,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ContractDeploymentRequest {
    pub deployer: String,
    pub wasm_hex: String,
    pub manifest: DeploymentManifest,
    pub fee: u64,
    pub nonce: u64,
    pub timestamp: String,
    pub signature_hex: String,
    pub tx_id: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ContractDeploymentResponse {
    pub tx_id: String,
    pub status: String,
    pub contract_address: String,
    pub code_hash: String,
}
