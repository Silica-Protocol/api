//! Faucet request entity for tracking testnet token distributions.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "faucet_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Recipient wallet address
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub recipient_address: String,
    /// IP address of the requester (for rate limiting)
    #[sea_orm(column_type = "String(StringLen::N(45))")]
    pub ip_address: String,
    /// Amount of tokens sent (in base units)
    pub amount: i64,
    /// Transaction hash from the faucet drip
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub tx_hash: String,
    /// Timestamp of the request
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
