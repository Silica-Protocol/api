use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "wallet_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub identity_id: Vec<u8>,
    #[sea_orm(primary_key, auto_increment = false)]
    pub wallet_address: String,
    pub link_type: String,
    pub proof_signature: Vec<u8>,
    pub created_at: i64,
    pub verified_at: Option<i64>,
    pub last_synced_block: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::identity_profile::Entity",
        from = "Column::IdentityId",
        to = "super::identity_profile::Column::IdentityId"
    )]
    IdentityProfile,
}

impl Related<super::identity_profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::IdentityProfile.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
