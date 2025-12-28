use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalView {
    pub proposal_id: i64,
    pub proposer: String,
    pub targets: Vec<String>,
    pub values: Vec<String>,
    pub calldatas: Vec<String>,
    pub description: String,
    pub vote_start: i64,
    pub vote_end: i64,
    pub votes_for: i64,
    pub votes_against: i64,
    pub votes_abstain: i64,
    pub state: String,
    pub executed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub has_voted: Option<bool>,
    pub user_vote: Option<VoteView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteView {
    pub proposal_id: i64,
    pub voter: String,
    pub support: i32, // 0=Against, 1=For, 2=Abstain
    pub weight: i64,
    pub reason: Option<String>,
    pub voted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationView {
    pub delegator: String,
    pub delegatee: String,
    pub amount: i64,
    pub delegated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VotingPowerView {
    pub address: String,
    pub voting_power: i64,
    pub delegated_power: i64,
    pub total_power: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalSummary {
    pub proposal_id: i64,
    pub proposer: String,
    pub description: String,
    pub vote_start: i64,
    pub vote_end: i64,
    pub votes_for: i64,
    pub votes_against: i64,
    pub votes_abstain: i64,
    pub state: String,
    pub created_at: i64,
}

// Request/Response types for governance HTTP API

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegateRequest {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegateResponse {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub delegation: DelegationView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceStatsView {
    pub address: String,
    pub proposals_submitted: i64,
    pub votes_cast: i64,
    pub participation_rate: f64,
    pub last_vote_at: Option<i64>,
    pub delegated_in: i64,
    pub delegated_out: i64,
    pub net_voting_power: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalCreateRequest {
    pub proposer: String,
    pub title: String,
    pub description: String,
    pub justification: Option<String>,
    pub targets: Vec<String>,
    pub values: Vec<String>,
    pub calldatas: Vec<String>,
    pub vote_duration_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteHistoryEntry {
    pub proposal_id: i64,
    pub support: i32,
    pub weight: i64,
    pub reason: Option<String>,
    pub voted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteSubmissionRequest {
    pub proposal_id: String, // Can be numeric string identifier
    pub voter: String,
    pub support: Option<i32>,   // 0=Against, 1=For, 2=Abstain
    pub option: Option<String>, // Alternative: "yes", "no", "abstain", etc.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteSubmissionResponse {
    pub proposal_id: i64,
    pub status: String,
    pub votes_for: i64,
    pub votes_against: i64,
    pub voter: String,
    pub vote_weight: i64,
    pub approve: bool,
    pub finalized: bool,
}
