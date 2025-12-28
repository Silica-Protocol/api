use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "stealth_outputs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub tx_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub output_index: i32,
    pub block_number: i64,
    pub sender: String,
    pub fee: i64,
    pub timestamp: DateTimeWithTimeZone,
    pub commitment: Vec<u8>,
    pub stealth_public_key: Vec<u8>,
    pub tx_public_key: Vec<u8>,
    pub amount: Option<i64>,
    pub memo_plaintext: Option<String>,
    pub encrypted_memo_ciphertext: Option<Vec<u8>>,
    pub encrypted_memo_nonce: Option<Vec<u8>>,
    pub encrypted_memo_message_number: Option<i32>,
    pub output_created_at: DateTimeWithTimeZone,
    pub inserted_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::chain_transaction::Entity",
        from = "Column::TxId",
        to = "super::chain_transaction::Column::TxId",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    ChainTransaction,
}

impl Related<super::chain_transaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChainTransaction.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
