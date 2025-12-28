use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StealthAddressRequestPayload {
    #[serde(default)]
    pub seed_hex: Option<String>,
    #[serde(default)]
    pub include_secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthAddressResponsePayload {
    pub address: String,
    pub view_key: String,
    pub spend_public_key: String,
    #[serde(default)]
    pub view_secret: Option<String>,
    #[serde(default)]
    pub spend_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthKeyComponentPayload {
    pub public: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthKeyBundlePayload {
    pub view_keypair: StealthKeyComponentPayload,
    pub spend_keypair: StealthKeyComponentPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthScanRequestPayload {
    pub stealth_keys: StealthKeyBundlePayload,
    #[serde(default)]
    pub from_block: Option<u64>,
    #[serde(default)]
    pub to_block: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthScanRangeSummary {
    pub from_block: u64,
    pub to_block: u64,
    pub span: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthAddressObservation {
    pub public_key: String,
    pub tx_public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedStealthTransactionView {
    pub transaction_id: String,
    pub sender: String,
    pub fee: u64,
    pub amount: u64,
    pub timestamp: Value,
    pub stealth_address: StealthAddressObservation,
    #[serde(default)]
    pub memo: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthScanResponsePayload {
    pub range: StealthScanRangeSummary,
    pub latest_block: u64,
    pub total_scanned: u64,
    pub total_balance: u64,
    pub transactions_returned: usize,
    pub has_more: bool,
    pub transactions: Vec<OwnedStealthTransactionView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthTransferRequestPayload {
    pub sender_keys: StealthKeyBundlePayload,
    pub recipient_view_key: String,
    pub recipient_spend_key: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    #[serde(default)]
    pub memo: Option<String>,
    #[serde(default)]
    pub privacy_level: StealthPrivacyLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthTransferResponsePayload {
    pub tx_hash: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StealthPrivacyLevel {
    Stealth,
    #[default]
    Encrypted,
}

impl StealthPrivacyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            StealthPrivacyLevel::Stealth => "stealth",
            StealthPrivacyLevel::Encrypted => "encrypted",
        }
    }
}
