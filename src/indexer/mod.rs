use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Datelike, FixedOffset, Utc};
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue::Set;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::DatabaseTransaction;
use sea_orm::EntityTrait;
use sea_orm::IntoActiveModel;
use sea_orm::QueryFilter;
use sea_orm::TransactionTrait;
use silica_models::stealth::STEALTH_OUTPUT_MEMO_MAX_BYTES;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use silica::execution::MAX_STEALTH_OUTPUTS_PER_TRANSACTION;
use silica::types::{Block, Transaction, TransactionType};

use crate::config::IndexerConfig;
use crate::entities::chain_block;
use crate::entities::chain_transaction;
use crate::entities::identity_profile;
use crate::entities::indexer_checkpoint;
use crate::entities::prelude::*;
use crate::entities::stealth_output;
use crate::entities::wallet_link;
use crate::identity::{
    AVATAR_HASH_BYTES, MAX_WALLET_LINKS, canonicalize_bio, canonicalize_display_name,
    decode_hex_with_expected, decode_identity_id, decode_signature, display_name_search_key,
    encode_identity_id, normalize_link_type, normalize_visibility, sanitize_wallet_address,
};
use crate::rpc::{IdentityRecord, IdentityRegistryResponse, RpcClient, WalletLinkRecord};
use crate::state::ApiCache;

const CHAIN_CHECKPOINT_ID: &str = "chain";
const IDENTITY_CHECKPOINT_ID: &str = "identity_registry";
const MAX_IDENTITY_SYNC_ITERATIONS: usize = 2048;

pub struct ChainIndexer {
    database: DatabaseConnection,
    rpc: RpcClient,
    config: IndexerConfig,
    last_indexed_block: Arc<AtomicU64>,
    cache: Arc<ApiCache>,
}

impl ChainIndexer {
    pub fn new(
        database: DatabaseConnection,
        rpc: RpcClient,
        config: IndexerConfig,
        last_indexed_block: Arc<AtomicU64>,
        cache: Arc<ApiCache>,
    ) -> Self {
        assert!(config.batch_size > 0, "Indexer batch size must be positive");
        assert!(
            Arc::strong_count(&last_indexed_block) >= 1,
            "Indexer state must be shared"
        );
        Self {
            database,
            rpc,
            config,
            last_indexed_block,
            cache,
        }
    }

    pub async fn run(mut self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        info!("Starting chain indexer loop");
        let mut checkpoint = self.load_checkpoint().await?;
        self.last_indexed_block
            .store(checkpoint, AtomicOrdering::SeqCst);
        let _ = self.load_checkpoint_for(IDENTITY_CHECKPOINT_ID).await?;

        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    match changed {
                        Ok(_) => {
                            if *shutdown.borrow() {
                                info!("Indexer shutdown signal received");
                                break;
                            }
                        }
                        Err(_) => {
                            warn!("Shutdown channel closed unexpectedly. Exiting indexer loop");
                            break;
                        }
                    }
                }
                _ = sleep(self.config.poll_interval()) => {
                    checkpoint = self.tick(checkpoint).await?;
                }
            }
        }

        Ok(())
    }

    async fn tick(&mut self, current: u64) -> Result<u64> {
        let latest = self.rpc.fetch_latest_block_number().await?;
        assert!(latest >= current, "Chain height must not regress");
        assert!(
            latest <= i64::MAX as u64,
            "Latest block exceeds storage capacity"
        );

        if latest == current {
            debug!("Indexer up to date at block {current}");
            return Ok(current);
        }

        let mut blocks = self.rpc.fetch_blocks().await?;
        blocks.retain(|block| block.block_number > current);
        blocks.sort_by(|a, b| a.block_number.cmp(&b.block_number));

        let mut processed = current;
        for block in blocks {
            let block_number = block.block_number;
            if block_number <= current {
                continue;
            }
            self.persist_block(&block).await?;
            processed = block_number;
            self.last_indexed_block
                .store(processed, AtomicOrdering::SeqCst);
        }

        if processed > current {
            self.persist_checkpoint_for(CHAIN_CHECKPOINT_ID, processed)
                .await?;
            self.sync_identity_registry(processed).await?;
        }

        Ok(processed)
    }

    async fn load_checkpoint(&self) -> Result<u64> {
        self.load_checkpoint_for(CHAIN_CHECKPOINT_ID).await
    }

    async fn load_checkpoint_for(&self, id: &str) -> Result<u64> {
        assert!(!id.is_empty(), "Checkpoint identifier cannot be empty");
        let maybe_checkpoint = IndexerCheckpoint::find_by_id(id.to_string())
            .one(&self.database)
            .await
            .with_context(|| format!("Failed to query indexer checkpoint {id}"))?;

        if let Some(record) = maybe_checkpoint {
            assert!(record.last_block_number >= 0, "Negative checkpoint stored");
            return Ok(record.last_block_number as u64);
        }

        self.create_checkpoint(id, 0).await?;
        Ok(0)
    }

    async fn create_checkpoint(&self, id: &str, block: u64) -> Result<()> {
        assert!(!id.is_empty(), "Checkpoint identifier cannot be empty");
        assert!(
            block <= i64::MAX as u64,
            "Checkpoint block exceeds i64 bounds"
        );
        assert!(
            block < 1_000_000_000_000,
            "Checkpoint initialization exceeded bound"
        );
        let now = fixed_now();
        let checkpoint = indexer_checkpoint::ActiveModel {
            id: Set(id.to_string()),
            last_block_number: Set(block as i64),
            updated_at: Set(now),
        };
        checkpoint
            .insert(&self.database)
            .await
            .with_context(|| format!("Failed to initialize checkpoint {id}"))?;
        Ok(())
    }

    async fn persist_checkpoint_for(&self, id: &str, block: u64) -> Result<()> {
        assert!(!id.is_empty(), "Checkpoint identifier cannot be empty");
        assert!(block <= i64::MAX as u64, "Checkpoint block exceeds limit");
        assert!(block < 1_000_000_000_000, "Checkpoint sanity exceeded");

        let now = fixed_now();
        let mut checkpoint = indexer_checkpoint::Entity::find_by_id(id.to_string())
            .one(&self.database)
            .await?
            .map(|model| model.into_active_model())
            .unwrap_or_else(|| indexer_checkpoint::ActiveModel {
                id: Set(id.to_string()),
                last_block_number: Set(0),
                updated_at: Set(now),
            });

        checkpoint.last_block_number = Set(block as i64);
        checkpoint.updated_at = Set(now);
        checkpoint
            .save(&self.database)
            .await
            .with_context(|| format!("Failed to update checkpoint {id}"))?;
        Ok(())
    }

    async fn persist_block(&self, block: &Block) -> Result<()> {
        let block_number = i64::try_from(block.block_number)
            .map_err(|_| anyhow!("Block number {} overflows i64", block.block_number))?;
        assert!(block_number >= 0, "Block number negative after conversion");

        if chain_block::Entity::find_by_id(block_number)
            .one(&self.database)
            .await?
            .is_some()
        {
            return Ok(());
        }

        let txn = self.database.begin().await?;
        self.insert_block(&txn, block).await?;
        self.insert_transactions(&txn, block).await?;
        txn.commit().await?;
        Ok(())
    }

    async fn insert_block(&self, txn: &DatabaseTransaction, block: &Block) -> Result<()> {
        let block_number = i64::try_from(block.block_number)
            .map_err(|_| anyhow!("Block number {} overflows i64", block.block_number))?;
        let now = fixed_now();
        let timestamp = to_fixed_offset(block.timestamp);
        assert!(!block.block_hash.is_empty(), "Block hash cannot be empty");
        assert_eq!(block.state_root.len(), 32, "State root must be 32 bytes");
        assert!(
            block.gas_limit <= i64::MAX as u64,
            "Gas limit exceeds i64 bounds"
        );
        assert!(
            block.gas_used <= i64::MAX as u64,
            "Gas used exceeds i64 bounds"
        );
        assert!(
            block.gas_used <= block.gas_limit,
            "Gas used exceeds gas limit"
        );
        assert!(
            block.state_leaf_count <= i64::MAX as u64,
            "State leaf count exceeds i64 bounds"
        );
        assert!(
            block.transactions.len() <= i32::MAX as usize,
            "Transaction count exceeds i32 bounds"
        );

        let model = chain_block::ActiveModel {
            block_number: Set(block_number),
            block_hash: Set(block.block_hash.clone()),
            previous_block_hash: Set(block.previous_block_hash.clone()),
            timestamp: Set(timestamp),
            validator_address: Set(block.validator_address.clone()),
            gas_used: Set(i64::try_from(block.gas_used)
                .map_err(|_| anyhow!("Gas used {} overflows i64", block.gas_used))?),
            gas_limit: Set(i64::try_from(block.gas_limit)
                .map_err(|_| anyhow!("Gas limit {} overflows i64", block.gas_limit))?),
            state_root: Set(block.state_root.to_vec()),
            state_leaf_count: Set(i64::try_from(block.state_leaf_count).map_err(|_| {
                anyhow!("State leaf count {} overflows i64", block.state_leaf_count)
            })?),
            tx_count: Set(i32::try_from(block.transactions.len()).map_err(|_| {
                anyhow!(
                    "Transaction count {} overflows i32",
                    block.transactions.len()
                )
            })?),
            indexed_at: Set(now),
            received_at: Set(now),
        };

        model
            .insert(txn)
            .await
            .with_context(|| format!("Failed to insert block {}", block.block_number))?;
        Ok(())
    }

    async fn insert_transactions(&self, txn: &DatabaseTransaction, block: &Block) -> Result<()> {
        assert!(
            block.transactions.len() <= 10_000,
            "Block transaction fan-out too large"
        );
        assert!(
            block.block_number <= i64::MAX as u64,
            "Block number exceeds bounds"
        );
        for transaction in &block.transactions {
            self.insert_transaction(txn, block, transaction).await?;
        }
        Ok(())
    }

    async fn insert_transaction(
        &self,
        txn: &DatabaseTransaction,
        block: &Block,
        transaction: &Transaction,
    ) -> Result<()> {
        assert!(
            transaction.amount <= i64::MAX as u64,
            "Transaction amount exceeds i64 bounds"
        );
        assert!(
            transaction.fee <= i64::MAX as u64,
            "Transaction fee exceeds bounds"
        );

        let tx_id = transaction.tx_id.clone();
        if chain_transaction::Entity::find_by_id(&tx_id)
            .one(txn)
            .await?
            .is_some()
        {
            return Ok(());
        }

        let kind = describe_transaction_type(transaction.transaction_type());
        let json_payload = serde_json::to_value(transaction).map_err(|err| {
            anyhow!(
                "Failed to serialize transaction {}: {err}",
                transaction.tx_id
            )
        })?;

        let model = chain_transaction::ActiveModel {
            tx_id: Set(tx_id),
            block_number: Set(i64::try_from(block.block_number)
                .map_err(|_| anyhow!("Block number {} overflows i64", block.block_number))?),
            sender: Set(transaction.sender.clone()),
            recipient: Set(transaction.recipient.clone()),
            amount: Set(i64::try_from(transaction.amount)
                .map_err(|_| anyhow!("Transaction amount {} overflows i64", transaction.amount))?),
            fee: Set(i64::try_from(transaction.fee)
                .map_err(|_| anyhow!("Transaction fee {} overflows i64", transaction.fee))?),
            nonce: Set(i64::try_from(transaction.nonce)
                .map_err(|_| anyhow!("Transaction nonce {} overflows i64", transaction.nonce))?),
            timestamp: Set(to_fixed_offset(transaction.timestamp)),
            transaction_type: Set(kind.to_string()),
            payload: Set(json_payload),
            indexed_at: Set(fixed_now()),
        };

        model
            .insert(txn)
            .await
            .with_context(|| format!("Failed to insert transaction {}", transaction.tx_id))?;
        self.insert_stealth_outputs(txn, block, transaction).await?;
        Ok(())
    }

    async fn insert_stealth_outputs(
        &self,
        txn: &DatabaseTransaction,
        block: &Block,
        transaction: &Transaction,
    ) -> Result<()> {
        if transaction.stealth_outputs.is_empty() {
            return Ok(());
        }

        assert!(
            transaction.stealth_outputs.len() <= MAX_STEALTH_OUTPUTS_PER_TRANSACTION,
            "Stealth output batch exceeds defensive bound"
        );

        let block_number = i64::try_from(block.block_number)
            .map_err(|_| anyhow!("Block number {} overflows i64", block.block_number))?;
        let tx_timestamp = to_fixed_offset(transaction.timestamp);

        let mut models = Vec::with_capacity(transaction.stealth_outputs.len());
        for (position, output) in transaction.stealth_outputs.iter().enumerate() {
            assert!(
                position < MAX_STEALTH_OUTPUTS_PER_TRANSACTION,
                "Stealth output iteration exceeded defensive bound"
            );

            let output_index = i32::try_from(output.index)
                .map_err(|_| anyhow!("Stealth output index {} overflows i32", output.index))?;

            if let Some(memo) = &output.memo_plaintext {
                assert!(
                    memo.len() <= STEALTH_OUTPUT_MEMO_MAX_BYTES,
                    "Plaintext memo exceeds {} byte bound",
                    STEALTH_OUTPUT_MEMO_MAX_BYTES
                );
            }

            let amount =
                match output.amount {
                    Some(value) => {
                        assert!(value > 0, "Stealth output amount must be positive");
                        Some(i64::try_from(value).map_err(|_| {
                            anyhow!("Stealth output amount {} overflows i64", value)
                        })?)
                    }
                    None => None,
                };

            let encrypted_fields = output
                .memo_encrypted
                .as_ref()
                .map(|memo| {
                    if memo.ciphertext.is_empty() {
                        return Err(anyhow!("Encrypted memo ciphertext cannot be empty"));
                    }
                    let message_number = i32::try_from(memo.message_number).map_err(|_| {
                        anyhow!(
                            "Encrypted memo message number {} exceeds i32 bounds",
                            memo.message_number
                        )
                    })?;
                    Ok((memo.ciphertext.clone(), memo.nonce.to_vec(), message_number))
                })
                .transpose()?;

            let (ciphertext, nonce, message_number) = match encrypted_fields {
                Some(fields) => (Some(fields.0), Some(fields.1), Some(fields.2)),
                None => (None, None, None),
            };

            let output_created_at = to_fixed_offset(output.created_at);
            let inserted_at = fixed_now();

            assert!(
                transaction.sender.len() <= 128,
                "Sender address exceeds defensive length bound"
            );

            let model = stealth_output::ActiveModel {
                tx_id: Set(transaction.tx_id.clone()),
                output_index: Set(output_index),
                block_number: Set(block_number),
                sender: Set(transaction.sender.clone()),
                fee: Set(i64::try_from(transaction.fee)
                    .map_err(|_| anyhow!("Transaction fee {} overflows i64", transaction.fee))?),
                timestamp: Set(tx_timestamp),
                commitment: Set(output.commitment.to_vec()),
                stealth_public_key: Set(output.address.public_key.to_vec()),
                tx_public_key: Set(output.address.tx_public_key.to_vec()),
                amount: Set(amount),
                memo_plaintext: Set(output.memo_plaintext.clone()),
                encrypted_memo_ciphertext: Set(ciphertext),
                encrypted_memo_nonce: Set(nonce),
                encrypted_memo_message_number: Set(message_number),
                output_created_at: Set(output_created_at),
                inserted_at: Set(inserted_at),
            };

            models.push(model);
        }

        assert!(
            models.len() <= MAX_STEALTH_OUTPUTS_PER_TRANSACTION,
            "Stealth output aggregation breached defensive bound"
        );

        stealth_output::Entity::insert_many(models)
            .exec(txn)
            .await
            .context("Failed to persist stealth outputs")?;

        Ok(())
    }

    async fn sync_identity_registry(&self, chain_tip: u64) -> Result<()> {
        let mut checkpoint = self.load_checkpoint_for(IDENTITY_CHECKPOINT_ID).await?;
        if checkpoint >= chain_tip {
            return Ok(());
        }

        let mut iterations = 0usize;
        let batch_size = self.config.identity_batch_size();
        assert!(batch_size > 0, "Identity batch size must be positive");

        while checkpoint < chain_tip {
            iterations += 1;
            assert!(
                iterations <= MAX_IDENTITY_SYNC_ITERATIONS,
                "Identity registry sync exceeded iteration bound"
            );

            let response = self
                .rpc
                .fetch_identity_registry(checkpoint, batch_size)
                .await?;

            let next_checkpoint = self.apply_identity_updates(checkpoint, &response).await?;

            if next_checkpoint <= checkpoint {
                // No progress reported by RPC, avoid infinite loop.
                break;
            }
            checkpoint = next_checkpoint;
        }

        Ok(())
    }

    async fn apply_identity_updates(
        &self,
        previous_checkpoint: u64,
        response: &IdentityRegistryResponse,
    ) -> Result<u64> {
        assert!(
            response.latest_block >= previous_checkpoint,
            "Identity registry checkpoint regressed"
        );
        assert!(
            response.latest_block <= i64::MAX as u64,
            "Identity registry latest block exceeds bounds"
        );

        if response.updates.is_empty() {
            self.persist_checkpoint_for(IDENTITY_CHECKPOINT_ID, response.latest_block)
                .await?;
            return Ok(response.latest_block);
        }

        let txn = self.database.begin().await?;
        for (index, update) in response.updates.iter().enumerate() {
            assert!(
                index < MAX_IDENTITY_SYNC_ITERATIONS,
                "Identity update loop exceeded defensive bound"
            );
            self.persist_identity_update(&txn, update).await?;
        }
        txn.commit().await?;

        self.persist_checkpoint_for(IDENTITY_CHECKPOINT_ID, response.latest_block)
            .await?;

        // Identity updates can invalidate cached search responses across queries.
        self.cache.identity_search.invalidate_all();

        Ok(response.latest_block)
    }

    async fn persist_identity_update(
        &self,
        txn: &DatabaseTransaction,
        update: &IdentityRecord,
    ) -> Result<()> {
        let identity_bytes = decode_identity_id(&update.identity_id)
            .with_context(|| format!("Invalid identity id {}", update.identity_id))?;
        let canonical_id = encode_identity_id(&identity_bytes);

        let display_name = match update.display_name.as_deref() {
            Some(name) => canonicalize_display_name(name)?,
            None => None,
        };
        let display_name_search = display_name.as_deref().and_then(display_name_search_key);

        let avatar_hash = match update.avatar_hash.as_deref() {
            Some(hash) => Some(decode_hex_with_expected(
                hash,
                AVATAR_HASH_BYTES,
                "avatar hash",
            )?),
            None => None,
        };

        let bio = match update.bio.as_deref() {
            Some(text) => canonicalize_bio(text)?,
            None => None,
        };

        let visibility = normalize_visibility(&update.stats_visibility)?;

        assert!(
            update.created_at <= i64::MAX as u64,
            "created_at exceeds bounds"
        );
        assert!(
            update.updated_at <= i64::MAX as u64,
            "updated_at exceeds bounds"
        );
        assert!(
            update.updated_at_block <= i64::MAX as u64,
            "updated_at_block exceeds bounds"
        );

        let created_at = update.created_at as i64;
        let updated_at = update.updated_at as i64;
        let last_synced_block = update.updated_at_block as i64;
        let profile_version = i32::try_from(update.profile_version.unwrap_or(1))
            .map_err(|_| anyhow!("profile_version exceeds i32 limits"))?;

        let existing = identity_profile::Entity::find_by_id(identity_bytes.clone())
            .one(txn)
            .await?
            .map(identity_profile::ActiveModel::from)
            .unwrap_or_else(|| identity_profile::ActiveModel {
                identity_id: Set(identity_bytes.clone()),
                ..Default::default()
            });

        let mut model = existing;
        model.identity_id = Set(identity_bytes.clone());
        model.display_name = Set(display_name.clone());
        model.display_name_search = Set(display_name_search);
        model.avatar_hash = Set(avatar_hash.clone());
        model.bio = Set(bio.clone());
        model.stats_visibility = Set(visibility.to_string());
        model.created_at = Set(created_at);
        model.updated_at = Set(updated_at);
        model.last_synced_block = Set(last_synced_block);
        model.profile_version = Set(profile_version);

        model
            .save(txn)
            .await
            .with_context(|| format!("Failed to persist identity profile {canonical_id}"))?;

        self.refresh_wallet_links(txn, &identity_bytes, &canonical_id, &update.wallet_links)
            .await?;

        self.cache.identity_profiles.invalidate(&canonical_id).await;
        self.cache.identity_wallets.invalidate(&canonical_id).await;

        Ok(())
    }

    async fn refresh_wallet_links(
        &self,
        txn: &DatabaseTransaction,
        identity_bytes: &[u8],
        canonical_id: &str,
        links: &[WalletLinkRecord],
    ) -> Result<()> {
        assert!(
            links.len() <= MAX_WALLET_LINKS,
            "Wallet link batch exceeds limit"
        );

        wallet_link::Entity::delete_many()
            .filter(wallet_link::Column::IdentityId.eq(identity_bytes.to_vec()))
            .exec(txn)
            .await
            .with_context(|| {
                format!("Failed to delete existing wallet links for {canonical_id}")
            })?;

        if links.is_empty() {
            return Ok(());
        }

        let mut models = Vec::with_capacity(links.len());
        for (index, link) in links.iter().enumerate() {
            assert!(
                index < MAX_WALLET_LINKS,
                "Wallet link iteration exceeded bound"
            );
            assert!(
                link.updated_at_block <= i64::MAX as u64,
                "Wallet link updated_at_block exceeds bounds"
            );
            assert!(
                link.created_at <= i64::MAX as u64,
                "Wallet link created_at exceeds bounds"
            );
            if let Some(verified_at) = link.verified_at {
                assert!(
                    verified_at <= i64::MAX as u64,
                    "Wallet link verified_at exceeds bounds"
                );
            }

            let wallet_address = sanitize_wallet_address(&link.wallet_address)?;
            let link_type = normalize_link_type(&link.link_type)?.into_owned();
            let signature = decode_signature(&link.proof_signature)?;
            assert!(
                !signature.is_empty(),
                "Wallet proof signature cannot be empty"
            );

            let created_at = link.created_at as i64;
            let verified_at = link.verified_at.map(|ts| ts as i64);
            let last_synced_block = link.updated_at_block as i64;

            models.push(wallet_link::ActiveModel {
                identity_id: Set(identity_bytes.to_vec()),
                wallet_address: Set(wallet_address),
                link_type: Set(link_type),
                proof_signature: Set(signature),
                created_at: Set(created_at),
                verified_at: Set(verified_at),
                last_synced_block: Set(last_synced_block),
            });
        }

        wallet_link::Entity::insert_many(models)
            .exec(txn)
            .await
            .with_context(|| format!("Failed to persist wallet links for {canonical_id}"))?;

        Ok(())
    }
}

fn describe_transaction_type(transaction_type: TransactionType) -> &'static str {
    match transaction_type {
        TransactionType::Consensus => "consensus",
        TransactionType::SmartContract => "smart_contract",
        TransactionType::Transfer => "transfer",
        TransactionType::Governance => "governance",
        TransactionType::Staking => "staking",
        TransactionType::ContractDeployment => "contract_deployment",
        TransactionType::CrossShard => "cross_shard",
        TransactionType::Finality => "finality",
    }
}

fn to_fixed_offset(time: DateTime<Utc>) -> DateTime<FixedOffset> {
    let offset = FixedOffset::east_opt(0).unwrap();
    let converted = time.with_timezone(&offset);
    assert_eq!(
        converted.offset().local_minus_utc(),
        0,
        "Offset conversion failed"
    );
    assert!(converted.year() >= 1970, "Timestamp predates Unix epoch");
    converted
}

fn fixed_now() -> DateTime<FixedOffset> {
    to_fixed_offset(Utc::now())
}
