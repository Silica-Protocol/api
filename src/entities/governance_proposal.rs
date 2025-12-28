use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "governance_proposals")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub proposal_id: i64,
    pub proposer: String,
    pub targets: Json,
    pub values: Json,
    pub calldatas: Json,
    pub description: String,
    pub vote_start: i64,
    pub vote_end: i64,
    pub votes_for: i64,
    pub votes_against: i64,
    pub votes_abstain: i64,
    pub state: String,
    pub executed_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::governance_vote::Entity")]
    GovernanceVote,
}

impl Related<super::governance_vote::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GovernanceVote.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
