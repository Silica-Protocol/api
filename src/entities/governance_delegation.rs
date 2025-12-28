use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "governance_delegations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub delegator: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub delegatee: String,
    pub amount: i64,
    pub delegated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
