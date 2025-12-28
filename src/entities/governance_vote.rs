use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "governance_votes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub proposal_id: i64,
    pub voter: String,
    pub support: i32, // 0=Against, 1=For, 2=Abstain
    pub weight: i64,
    pub reason: Option<String>,
    pub voted_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::governance_proposal::Entity",
        from = "Column::ProposalId",
        to = "super::governance_proposal::Column::ProposalId"
    )]
    GovernanceProposal,
}

impl Related<super::governance_proposal::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GovernanceProposal.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
