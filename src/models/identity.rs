use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdentityProfileView {
    pub identity_id: String,
    pub display_name: Option<String>,
    pub avatar_hash: Option<String>,
    pub bio: Option<String>,
    pub stats_visibility: String,
    pub wallet_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_synced_block: i64,
    pub profile_version: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WalletLinkView {
    pub wallet_address: String,
    pub link_type: String,
    pub proof_signature: String,
    pub created_at: i64,
    pub verified_at: Option<i64>,
    pub last_synced_block: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdentitySearchResult {
    pub identity_id: String,
    pub display_name: Option<String>,
    pub stats_visibility: String,
    pub updated_at: i64,
}
