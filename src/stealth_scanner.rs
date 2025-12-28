use std::convert::TryFrom;

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder,
};
use serde_json::Value;
use silica::privacy::stealth::OwnedTransaction;
use silica::privacy::transactions::StealthTransaction;
use silica::privacy::{DoubleRatchetState, EncryptedPayload, StealthAddress, StealthKeyPair};
use silica_models::stealth::StealthAddressView;
use tracing::warn;

use crate::entities::stealth_output;
use crate::models::privacy::{OwnedStealthTransactionView, StealthAddressObservation};

const MAX_OUTPUTS_PER_REQUEST: u64 = 200_000;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("database error: {0}")]
    Database(#[from] DbErr),
    #[error("block number {block} exceeds storage bounds")]
    BlockBoundExceeded { block: u64 },
    #[error(
        "requested range returned {observed} stealth outputs which exceeds the defensive bound of {limit}"
    )]
    OutputOverflow { observed: u64, limit: u64 },
}

#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub transactions: Vec<OwnedStealthTransactionView>,
    pub owned_total: usize,
    pub total_balance: u64,
    pub total_scanned: usize,
    pub has_more: bool,
}

impl ScanOutcome {
    pub fn empty() -> Self {
        Self::default()
    }
}

pub async fn scan_owned_outputs(
    database: &DatabaseConnection,
    keys: &StealthKeyPair,
    from_block: u64,
    to_block: u64,
    limit: usize,
) -> Result<ScanOutcome, ScanError> {
    assert!(from_block <= to_block, "scan range must be ordered");
    let from_i64 = i64::try_from(from_block)
        .map_err(|_| ScanError::BlockBoundExceeded { block: from_block })?;
    let to_i64 =
        i64::try_from(to_block).map_err(|_| ScanError::BlockBoundExceeded { block: to_block })?;

    let range_condition = Condition::all()
        .add(stealth_output::Column::BlockNumber.gte(from_i64))
        .add(stealth_output::Column::BlockNumber.lte(to_i64));

    let total_outputs = stealth_output::Entity::find()
        .filter(range_condition.clone())
        .count(database)
        .await?;

    if total_outputs == 0 {
        return Ok(ScanOutcome::empty());
    }

    if total_outputs > MAX_OUTPUTS_PER_REQUEST {
        return Err(ScanError::OutputOverflow {
            observed: total_outputs,
            limit: MAX_OUTPUTS_PER_REQUEST,
        });
    }

    let models = stealth_output::Entity::find()
        .filter(range_condition)
        .order_by_asc(stealth_output::Column::BlockNumber)
        .order_by_asc(stealth_output::Column::OutputIndex)
        .all(database)
        .await?;

    let records = convert_models(&models);
    Ok(detect_owned_outputs(&records, keys, limit))
}

fn convert_models(models: &[stealth_output::Model]) -> Vec<StealthOutputRecord> {
    let mut records = Vec::with_capacity(models.len());
    for model in models {
        match StealthOutputRecord::try_from(model) {
            Ok(record) => records.push(record),
            Err(err) => warn!(
                tx_id = %model.tx_id,
                output_index = model.output_index,
                "Skipping malformed stealth output: {err}"
            ),
        }
    }
    records
}

fn detect_owned_outputs(
    records: &[StealthOutputRecord],
    keys: &StealthKeyPair,
    limit: usize,
) -> ScanOutcome {
    if records.is_empty() {
        return ScanOutcome::empty();
    }

    let mut outcome = ScanOutcome {
        transactions: Vec::with_capacity(limit.min(records.len())),
        owned_total: 0,
        total_balance: 0,
        total_scanned: records.len(),
        has_more: false,
    };

    for record in records {
        let maybe_view = match &record.kind {
            StoredOutputKind::Plaintext { amount, memo } => {
                evaluate_plaintext(record, *amount, memo, keys)
            }
            StoredOutputKind::Encrypted { memo } => evaluate_encrypted(record, memo, keys),
        };

        if let Some(view) = maybe_view {
            outcome.total_balance = outcome.total_balance.saturating_add(view.amount);
            outcome.owned_total += 1;
            if outcome.transactions.len() < limit {
                outcome.transactions.push(view);
            }
        }
    }

    outcome.has_more = outcome.owned_total > outcome.transactions.len();
    outcome
}

fn evaluate_plaintext(
    record: &StealthOutputRecord,
    amount: u64,
    memo: &Option<String>,
    keys: &StealthKeyPair,
) -> Option<OwnedStealthTransactionView> {
    let address = record.address.to_stealth_address().ok()?;
    let stealth_tx = StealthTransaction {
        tx_id: record.tx_id.clone(),
        sender: record.sender.clone(),
        stealth_address: address,
        amount,
        fee: record.fee,
        nonce: 0,
        timestamp: record.timestamp,
        signature: String::new(),
        memo: memo.clone(),
    };

    let owned: Option<OwnedTransaction> = keys
        .scan_for_transactions(std::slice::from_ref(&stealth_tx))
        .into_iter()
        .next();

    let owned = owned?;
    let decrypted_amount = owned.decrypted_amount.unwrap_or(amount);
    let memo_value = owned
        .decrypted_memo
        .as_ref()
        .or(stealth_tx.memo.as_ref())
        .cloned();

    Some(build_owned_view(record, decrypted_amount, memo_value))
}

fn evaluate_encrypted(
    record: &StealthOutputRecord,
    memo: &StoredEncryptedMemo,
    keys: &StealthKeyPair,
) -> Option<OwnedStealthTransactionView> {
    let address = record.address.to_stealth_address().ok()?;
    keys.owns_stealth_address(&address)?;

    let compressed = address.public_key.compress();
    let shared_secret = compressed.as_bytes();
    let mut ratchet = DoubleRatchetState::new_receiver(shared_secret);

    let payload = EncryptedPayload {
        ciphertext: memo.ciphertext.clone(),
        nonce: memo.nonce,
        message_number: memo.message_number,
    };

    let decrypted = ratchet.decrypt_payload(&payload).ok()?;
    Some(build_owned_view(record, decrypted.amount, decrypted.memo))
}

fn build_owned_view(
    record: &StealthOutputRecord,
    amount: u64,
    memo: Option<String>,
) -> OwnedStealthTransactionView {
    OwnedStealthTransactionView {
        transaction_id: record.tx_id.clone(),
        sender: record.sender.clone(),
        fee: record.fee,
        amount,
        timestamp: timestamp_value(&record.timestamp),
        stealth_address: record.address.observation.clone(),
        memo: memo.map(|value| memo_to_value(&value)),
    }
}

fn timestamp_value(timestamp: &DateTime<Utc>) -> Value {
    Value::String(timestamp.to_rfc3339())
}

fn memo_to_value(memo: &str) -> Value {
    serde_json::from_str(memo).unwrap_or_else(|_| Value::String(memo.to_string()))
}

struct StealthOutputRecord {
    tx_id: String,
    sender: String,
    fee: u64,
    timestamp: DateTime<Utc>,
    address: AddressRecord,
    kind: StoredOutputKind,
}

#[derive(Clone)]
struct AddressRecord {
    view: StealthAddressView,
    observation: StealthAddressObservation,
}

impl AddressRecord {
    fn to_stealth_address(&self) -> anyhow::Result<StealthAddress> {
        StealthAddress::from_view(&self.view)
            .map_err(|err| anyhow!("Invalid stealth address data: {err}"))
    }
}

enum StoredOutputKind {
    Plaintext { amount: u64, memo: Option<String> },
    Encrypted { memo: StoredEncryptedMemo },
}

#[derive(Clone)]
struct StoredEncryptedMemo {
    ciphertext: Vec<u8>,
    nonce: [u8; 12],
    message_number: u32,
}

impl TryFrom<&stealth_output::Model> for StealthOutputRecord {
    type Error = anyhow::Error;

    fn try_from(model: &stealth_output::Model) -> Result<Self, Self::Error> {
        let view = StealthAddressView {
            public_key: to_array::<32>(&model.stealth_public_key)?,
            tx_public_key: to_array::<32>(&model.tx_public_key)?,
        };
        let observation = StealthAddressObservation {
            public_key: hex::encode(view.public_key),
            tx_public_key: hex::encode(view.tx_public_key),
        };

        let address = AddressRecord { view, observation };

        let fee = u64::try_from(model.fee)
            .map_err(|_| anyhow!("Fee {} cannot be represented as u64", model.fee))?;
        let timestamp = model.timestamp.with_timezone(&Utc);

        let amount = match model.amount {
            Some(value) => Some(
                u64::try_from(value)
                    .map_err(|_| anyhow!("Amount {} cannot be represented as u64", value))?,
            ),
            None => None,
        };

        let encrypted_memo = match (
            model.encrypted_memo_ciphertext.as_ref(),
            model.encrypted_memo_nonce.as_ref(),
            model.encrypted_memo_message_number,
        ) {
            (Some(ciphertext), Some(nonce), Some(number)) => Some(StoredEncryptedMemo {
                ciphertext: ciphertext.clone(),
                nonce: to_array::<12>(nonce)?,
                message_number: u32::try_from(number).map_err(|_| {
                    anyhow!("Encrypted memo message number {number} exceeds u32 bounds")
                })?,
            }),
            (None, None, None) => None,
            _ => {
                return Err(anyhow!(
                    "Encrypted memo fields must all be present or all absent"
                ));
            }
        };

        let kind = match (amount, &model.memo_plaintext, &encrypted_memo) {
            (Some(value), _, None) => StoredOutputKind::Plaintext {
                amount: value,
                memo: model.memo_plaintext.clone(),
            },
            (None, _, Some(memo)) => StoredOutputKind::Encrypted { memo: memo.clone() },
            _ => {
                return Err(anyhow!(
                    "Stealth output row has inconsistent plaintext/encrypted data"
                ));
            }
        };

        Ok(Self {
            tx_id: model.tx_id.clone(),
            sender: model.sender.clone(),
            fee,
            timestamp,
            address,
            kind,
        })
    }
}

fn to_array<const N: usize>(value: &[u8]) -> Result<[u8; N], anyhow::Error> {
    if value.len() != N {
        return Err(anyhow!(
            "Expected {N} bytes but received {} bytes",
            value.len()
        ));
    }
    let mut array = [0u8; N];
    array.copy_from_slice(value);
    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use silica::privacy::PrivateTransactionPayload;

    fn address_record(address: &StealthAddress) -> AddressRecord {
        let view = address.to_view();
        let observation = StealthAddressObservation {
            public_key: hex::encode(view.public_key),
            tx_public_key: hex::encode(view.tx_public_key),
        };
        AddressRecord { view, observation }
    }

    #[test]
    fn detect_owned_plaintext_outputs_returns_view() {
        let recipient = StealthKeyPair::generate();
        let (address, _) = StealthKeyPair::generate_stealth_address(
            &recipient.view_keypair.public,
            &recipient.spend_keypair.public,
        );

        let record = StealthOutputRecord {
            tx_id: "tx_plain".to_string(),
            sender: "sender_alpha".to_string(),
            fee: 10,
            timestamp: Utc::now(),
            address: address_record(&address),
            kind: StoredOutputKind::Plaintext {
                amount: 42,
                memo: Some("{\"note\":\"hello\"}".to_string()),
            },
        };

        let records = vec![record];
        let outcome = detect_owned_outputs(&records, &recipient, 4);

        assert_eq!(outcome.owned_total, 1, "exactly one owned output detected");
        assert_eq!(outcome.total_balance, 42);
        assert!(!outcome.has_more);

        let view = outcome.transactions.first().expect("transaction returned");
        assert_eq!(view.amount, 42);
        assert_eq!(view.sender, "sender_alpha");
        assert_eq!(view.memo.as_ref().unwrap()["note"], "hello");
    }

    #[test]
    fn detect_owned_encrypted_outputs_decrypts_payload() {
        let recipient = StealthKeyPair::generate();
        let (address, _) = StealthKeyPair::generate_stealth_address(
            &recipient.view_keypair.public,
            &recipient.spend_keypair.public,
        );

        let payload = PrivateTransactionPayload {
            amount: 77,
            memo: Some("{\"note\":\"secret\"}".to_string()),
            fee: 3,
            timestamp: 1,
        };

        let compressed = address.public_key.compress();
        let shared_secret = compressed.as_bytes();
        let mut ratchet = DoubleRatchetState::new_sender(shared_secret);
        let encrypted = ratchet
            .encrypt_payload(&payload)
            .expect("encryption succeeds");

        let record = StealthOutputRecord {
            tx_id: "tx_encrypted".to_string(),
            sender: "sender_beta".to_string(),
            fee: payload.fee,
            timestamp: Utc::now(),
            address: address_record(&address),
            kind: StoredOutputKind::Encrypted {
                memo: StoredEncryptedMemo {
                    ciphertext: encrypted.ciphertext.clone(),
                    nonce: encrypted.nonce,
                    message_number: encrypted.message_number,
                },
            },
        };

        let records = vec![record];
        let outcome = detect_owned_outputs(&records, &recipient, 4);

        assert_eq!(outcome.owned_total, 1);
        assert_eq!(outcome.total_balance, payload.amount);

        let view = outcome.transactions.first().expect("transaction returned");
        assert_eq!(view.amount, payload.amount);
        assert_eq!(view.memo.as_ref().unwrap()["note"], "secret");
    }
}
