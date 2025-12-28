use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "chain_blocks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub block_number: i64,
    pub block_hash: String,
    pub previous_block_hash: String,
    pub timestamp: DateTimeWithTimeZone,
    pub validator_address: String,
    pub gas_used: i64,
    pub gas_limit: i64,
    pub state_root: Vec<u8>,
    pub state_leaf_count: i64,
    pub tx_count: i32,
    pub indexed_at: DateTimeWithTimeZone,
    pub received_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::chain_transaction::Entity")]
    ChainTransaction,
}

impl Related<super::chain_transaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChainTransaction.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
