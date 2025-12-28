use sea_orm::JsonValue;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "chain_transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub tx_id: String,
    pub block_number: i64,
    pub sender: String,
    pub recipient: String,
    pub amount: i64,
    pub fee: i64,
    pub nonce: i64,
    pub timestamp: DateTimeWithTimeZone,
    pub transaction_type: String,
    pub payload: JsonValue,
    pub indexed_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::chain_block::Entity",
        from = "Column::BlockNumber",
        to = "super::chain_block::Column::BlockNumber"
    )]
    ChainBlock,
}

impl Related<super::chain_block::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChainBlock.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
