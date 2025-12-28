use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "identity_profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub identity_id: Vec<u8>,
    pub display_name: Option<String>,
    pub display_name_search: Option<String>,
    pub avatar_hash: Option<Vec<u8>>,
    pub bio: Option<String>,
    pub stats_visibility: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_synced_block: i64,
    pub profile_version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::wallet_link::Entity")]
    WalletLink,
}

impl Related<super::wallet_link::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WalletLink.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
