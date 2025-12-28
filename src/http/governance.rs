use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::DateTime;
use sea_orm::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::Deserialize;

use crate::entities::{governance_delegation, governance_proposal, governance_vote};
use crate::models::governance::{
    DelegateRequest, DelegateResponse, DelegationView, GovernanceStatsView, ProposalCreateRequest,
    ProposalSummary, ProposalView, VoteHistoryEntry, VoteSubmissionRequest, VoteSubmissionResponse,
    VoteView, VotingPowerView,
};
use crate::state::AppState;

use super::HttpError;

const MAX_HISTORY_LIMIT: u64 = 500;
const MAX_PROPOSAL_QUERY_LIMIT: u64 = 100;
const MAX_PROPOSAL_VOTE_LIMIT: u64 = 500;
const MAX_DELEGATION_AMOUNT: u64 = 100_000_000_000_000;
const MAX_PROPOSAL_TITLE_LEN: usize = 256;
const MAX_PROPOSAL_DESCRIPTION_LEN: usize = 10_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proposals", get(get_proposals).post(create_proposal))
        .route("/proposals/:proposal_id", get(get_proposal))
        .route("/proposals/:proposal_id/votes", get(get_proposal_votes))
        .route("/votes/:address", get(get_vote_history))
        .route("/votes", post(submit_vote))
        .route("/voting-power/:address", get(get_voting_power))
        .route("/delegations/:address", get(get_delegations))
        .route("/delegations", post(delegate_voting_power))
        .route("/stats/:address", get(get_governance_stats))
}

#[derive(Debug, Deserialize)]
struct GetProposalsQuery {
    #[serde(alias = "status")]
    state: Option<String>,
    proposer: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ProposalDetailQuery {
    voter: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct VoteHistoryQuery {
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn get_proposals(
    Query(query): Query<GetProposalsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Vec<ProposalSummary>>, HttpError> {
    let requested_limit = query.limit.unwrap_or(50);
    if requested_limit == 0 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "limit must be positive".to_string(),
        ));
    }

    let limit = requested_limit.min(MAX_PROPOSAL_QUERY_LIMIT);
    let offset = query.offset.unwrap_or(0);
    assert!(limit > 0, "Proposal limit must be positive");
    assert!(
        offset <= i64::MAX as u64,
        "Proposal offset exceeds database bounds"
    );

    let mut select = governance_proposal::Entity::find();

    if let Some(state_filter) = query.state {
        select = select.filter(governance_proposal::Column::State.eq(state_filter));
    }

    if let Some(proposer) = query.proposer {
        select = select.filter(governance_proposal::Column::Proposer.eq(proposer));
    }

    let proposals = select
        .order_by_desc(governance_proposal::Column::CreatedAt)
        .limit(limit)
        .offset(offset)
        .all(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let summaries = proposals
        .into_iter()
        .map(|p| ProposalSummary {
            proposal_id: p.proposal_id,
            proposer: p.proposer,
            description: p.description,
            vote_start: p.vote_start,
            vote_end: p.vote_end,
            votes_for: p.votes_for,
            votes_against: p.votes_against,
            votes_abstain: p.votes_abstain,
            state: p.state,
            created_at: p.created_at.timestamp(),
        })
        .collect::<Vec<_>>();

    assert!(
        summaries.len() <= limit as usize,
        "Returned more proposals than requested",
    );

    Ok(Json(summaries))
}

async fn get_proposal(
    Path(proposal_id): Path<i64>,
    Query(detail): Query<ProposalDetailQuery>,
    State(state): State<AppState>,
) -> Result<Json<ProposalView>, HttpError> {
    assert!(proposal_id >= 0, "Proposal id must be non-negative");

    let proposal = governance_proposal::Entity::find_by_id(proposal_id)
        .one(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                format!("Proposal {proposal_id} not found"),
            )
        })?;

    let targets: Vec<String> = serde_json::from_value(proposal.targets.clone())
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let values: Vec<String> = serde_json::from_value(proposal.values.clone())
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let calldatas: Vec<String> = serde_json::from_value(proposal.calldatas.clone())
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    assert_eq!(
        targets.len(),
        values.len(),
        "Proposal targets and values must align",
    );
    assert_eq!(
        targets.len(),
        calldatas.len(),
        "Proposal targets and calldatas must align",
    );

    let user_vote_record = if let Some(voter) = detail.voter.as_ref() {
        governance_vote::Entity::find()
            .filter(governance_vote::Column::ProposalId.eq(proposal_id))
            .filter(governance_vote::Column::Voter.eq(voter.clone()))
            .one(&state.database)
            .await
            .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    } else {
        None
    };

    let (has_voted, user_vote) = match (detail.voter.as_ref(), user_vote_record) {
        (Some(_), Some(vote)) => (
            Some(true),
            Some(VoteView {
                proposal_id: vote.proposal_id,
                voter: vote.voter,
                support: vote.support,
                weight: vote.weight,
                reason: vote.reason,
                voted_at: vote.voted_at.timestamp(),
            }),
        ),
        (Some(_), None) => (Some(false), None),
        (None, _) => (None, None),
    };

    let view = ProposalView {
        proposal_id: proposal.proposal_id,
        proposer: proposal.proposer,
        targets,
        values,
        calldatas,
        description: proposal.description,
        vote_start: proposal.vote_start,
        vote_end: proposal.vote_end,
        votes_for: proposal.votes_for,
        votes_against: proposal.votes_against,
        votes_abstain: proposal.votes_abstain,
        state: proposal.state,
        executed_at: proposal.executed_at.map(|dt| dt.timestamp()),
        created_at: proposal.created_at.timestamp(),
        updated_at: proposal.updated_at.timestamp(),
        has_voted,
        user_vote,
    };

    Ok(Json(view))
}

async fn get_proposal_votes(
    Path(proposal_id): Path<i64>,
    Query(query): Query<GetProposalsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Vec<VoteView>>, HttpError> {
    assert!(proposal_id >= 0, "Proposal id must be non-negative");

    let requested_limit = query.limit.unwrap_or(100);
    if requested_limit == 0 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "limit must be positive".to_string(),
        ));
    }

    let limit = requested_limit.min(MAX_PROPOSAL_VOTE_LIMIT);
    let offset = query.offset.unwrap_or(0);
    assert!(limit > 0, "Vote limit must be positive");
    assert!(
        offset <= i64::MAX as u64,
        "Vote offset exceeds database bounds"
    );

    let votes = governance_vote::Entity::find()
        .filter(governance_vote::Column::ProposalId.eq(proposal_id))
        .order_by_desc(governance_vote::Column::VotedAt)
        .limit(limit)
        .offset(offset)
        .all(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let vote_views = votes
        .into_iter()
        .map(|v| VoteView {
            proposal_id: v.proposal_id,
            voter: v.voter,
            support: v.support,
            weight: v.weight,
            reason: v.reason,
            voted_at: v.voted_at.timestamp(),
        })
        .collect::<Vec<_>>();

    assert!(
        vote_views.len() <= limit as usize,
        "Returned more votes than requested",
    );

    Ok(Json(vote_views))
}

async fn get_vote_history(
    Path(address): Path<String>,
    Query(query): Query<VoteHistoryQuery>,
    State(state): State<AppState>,
) -> Result<Json<Vec<VoteHistoryEntry>>, HttpError> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "address must not be empty".to_string(),
        ));
    }

    let requested_limit = query.limit.unwrap_or(100);
    if requested_limit == 0 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "limit must be at least 1".to_string(),
        ));
    }

    let limit = requested_limit.min(MAX_HISTORY_LIMIT);
    let offset = query.offset.unwrap_or(0);
    assert!(limit > 0, "Vote history limit must be positive");
    assert!(limit <= MAX_HISTORY_LIMIT, "Vote history limit exceeded");
    assert!(
        offset <= i64::MAX as u64,
        "Vote history offset exceeds bounds"
    );

    let votes = governance_vote::Entity::find()
        .filter(governance_vote::Column::Voter.eq(address.clone()))
        .order_by_desc(governance_vote::Column::VotedAt)
        .limit(limit)
        .offset(offset)
        .all(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mut history = Vec::with_capacity(votes.len());
    for (index, vote) in votes.into_iter().enumerate() {
        assert!(
            index < MAX_HISTORY_LIMIT as usize,
            "Vote history bound exceeded"
        );
        history.push(VoteHistoryEntry {
            proposal_id: vote.proposal_id,
            support: vote.support,
            weight: vote.weight,
            reason: vote.reason,
            voted_at: vote.voted_at.timestamp(),
        });
    }

    assert!(
        history.len() <= limit as usize,
        "Vote history result exceeds requested limit",
    );

    Ok(Json(history))
}

async fn get_voting_power(
    Path(address): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<VotingPowerView>, HttpError> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "address must not be empty".to_string(),
        ));
    }

    assert!(address.len() <= 128, "Address exceeds defensive bound");

    let delegated_to =
        sum_delegations(governance_delegation::Column::Delegatee, &address, &state).await?;
    let delegated_out =
        sum_delegations(governance_delegation::Column::Delegator, &address, &state).await?;

    let net_power = delegated_to - delegated_out;
    let voting_power = if net_power < 0 { 0 } else { net_power };
    let total_power = delegated_to.saturating_add(delegated_out);

    assert!(voting_power >= 0, "Voting power cannot be negative");
    assert!(total_power >= 0, "Total voting power cannot be negative");

    let view = VotingPowerView {
        address,
        voting_power,
        delegated_power: delegated_out,
        total_power,
    };

    Ok(Json(view))
}

async fn get_delegations(
    Path(address): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<DelegationView>>, HttpError> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "address must not be empty".to_string(),
        ));
    }

    assert!(address.len() <= 128, "Address exceeds defensive bound");

    let delegations = governance_delegation::Entity::find()
        .filter(
            governance_delegation::Column::Delegator
                .eq(address.clone())
                .or(governance_delegation::Column::Delegatee.eq(address.clone())),
        )
        .order_by_desc(governance_delegation::Column::DelegatedAt)
        .all(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let delegation_views = delegations
        .into_iter()
        .map(|d| DelegationView {
            delegator: d.delegator,
            delegatee: d.delegatee,
            amount: d.amount,
            delegated_at: d.delegated_at.timestamp(),
        })
        .collect::<Vec<_>>();

    assert!(
        delegation_views.len() <= MAX_HISTORY_LIMIT as usize,
        "Delegation result exceeds defensive bound",
    );

    Ok(Json(delegation_views))
}

async fn create_proposal(
    State(_state): State<AppState>,
    Json(request): Json<ProposalCreateRequest>,
) -> Result<StatusCode, HttpError> {
    let proposer = request.proposer.trim();
    if proposer.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "proposer must not be empty".to_string(),
        ));
    }

    assert!(proposer.len() <= 128, "Proposer exceeds defensive bound");

    let title = request.title.trim();
    if title.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "title must not be empty".to_string(),
        ));
    }

    assert!(
        title.len() <= MAX_PROPOSAL_TITLE_LEN,
        "Proposal title exceeds defensive bound",
    );

    let description = request.description.trim();
    if description.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "description must not be empty".to_string(),
        ));
    }

    assert!(
        description.len() <= MAX_PROPOSAL_DESCRIPTION_LEN,
        "Proposal description exceeds defensive bound",
    );

    if let Some(justification) = request.justification.as_ref() {
        assert!(
            justification.trim().len() <= MAX_PROPOSAL_DESCRIPTION_LEN,
            "Proposal justification exceeds defensive bound",
        );
    }

    let message = "Proposal creation via API is not yet supported. Submit proposals through the on-chain governance portal.";

    Err(HttpError::new(
        StatusCode::NOT_IMPLEMENTED,
        message.to_string(),
    ))
}

async fn submit_vote(
    State(state): State<AppState>,
    Json(request): Json<VoteSubmissionRequest>,
) -> Result<Json<VoteSubmissionResponse>, HttpError> {
    let support_value = resolve_support_value(&request)?;
    assert!(support_value >= 0, "Support value must be non-negative");
    assert!(support_value <= 2, "Support value exceeds defined range");

    let voter = request.voter.trim();
    if voter.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "voter must not be empty".to_string(),
        ));
    }

    assert!(voter.len() <= 128, "Voter exceeds defensive bound");

    if support_value == 2 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "abstain is not supported by the current governance voting contract".to_string(),
        ));
    }

    let approve = support_value == 1;

    let proposal_identifier = request.proposal_id.trim();
    if proposal_identifier.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "proposal_id must not be empty".to_string(),
        ));
    }

    assert!(
        proposal_identifier.len() <= 128,
        "Proposal identifier exceeds defensive bound",
    );

    let proposal_id_numeric = proposal_identifier.parse::<i64>().map_err(|_| {
        HttpError::new(
            StatusCode::BAD_REQUEST,
            "proposal_id must be a numeric identifier".to_string(),
        )
    })?;

    assert!(
        proposal_id_numeric >= 0,
        "Proposal identifier must be non-negative"
    );

    let rpc_response = state
        .rpc
        .governance_cast_vote(proposal_identifier, voter, approve)
        .await
        .map_err(|err| HttpError::new(StatusCode::BAD_GATEWAY, err.to_string()))?;

    let votes_for = count_to_i64("votes_for", rpc_response.votes_for)?;
    let votes_against = count_to_i64("votes_against", rpc_response.votes_against)?;
    let vote_weight = count_to_i64("vote_weight", rpc_response.vote_weight)?;

    state.cache.proposals.invalidate_all();

    let response = VoteSubmissionResponse {
        proposal_id: proposal_id_numeric,
        status: rpc_response.status,
        votes_for,
        votes_against,
        voter: rpc_response.voter,
        vote_weight,
        approve: rpc_response.approve,
        finalized: rpc_response.finalized,
    };

    Ok(Json(response))
}

async fn delegate_voting_power(
    State(state): State<AppState>,
    Json(request): Json<DelegateRequest>,
) -> Result<Json<DelegateResponse>, HttpError> {
    assert!(
        request.amount <= MAX_DELEGATION_AMOUNT,
        "Delegation amount exceeds static upper bound",
    );
    assert!(request.amount > 0, "Delegation amount must be positive");

    let delegator = request.delegator.trim();
    let validator = request.validator.trim();

    if delegator.is_empty() || validator.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "delegator and validator must not be empty".to_string(),
        ));
    }

    if delegator == validator {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "delegator and validator must be different addresses".to_string(),
        ));
    }

    assert!(delegator.len() <= 128, "Delegator exceeds defensive bound");
    assert!(validator.len() <= 128, "Validator exceeds defensive bound");

    let rpc_response = state
        .rpc
        .governance_delegate_stake(delegator, validator, request.amount)
        .await
        .map_err(|err| HttpError::new(StatusCode::BAD_GATEWAY, err.to_string()))?;

    let delegated_timestamp =
        parse_rfc3339_timestamp("delegated_at", &rpc_response.delegation.delegated_at)?;
    let delegated_amount = count_to_i64("delegation amount", rpc_response.delegation.amount)?;

    let delegation_view = DelegationView {
        delegator: rpc_response.delegation.delegator,
        delegatee: rpc_response.delegation.validator,
        amount: delegated_amount,
        delegated_at: delegated_timestamp,
    };

    let response = DelegateResponse {
        delegator: rpc_response.delegator,
        validator: rpc_response.validator,
        amount: rpc_response.amount,
        delegation: delegation_view,
    };

    Ok(Json(response))
}

async fn get_governance_stats(
    Path(address): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<GovernanceStatsView>, HttpError> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "address must not be empty".to_string(),
        ));
    }

    assert!(address.len() <= 128, "Address exceeds defensive bound");

    let submitted_raw = governance_proposal::Entity::find()
        .filter(governance_proposal::Column::Proposer.eq(address.clone()))
        .count(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let votes_cast_raw = governance_vote::Entity::find()
        .filter(governance_vote::Column::Voter.eq(address.clone()))
        .count(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let total_proposals_raw = governance_proposal::Entity::find()
        .count(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let proposals_submitted = count_to_i64("proposals submitted", submitted_raw)?;
    let votes_cast = count_to_i64("votes cast", votes_cast_raw)?;

    let participation_rate = if total_proposals_raw == 0 {
        0.0
    } else {
        ((votes_cast_raw as f64) / (total_proposals_raw as f64)).min(1.0)
    };

    assert!(
        participation_rate >= 0.0,
        "Participation rate must be non-negative"
    );
    assert!(
        participation_rate <= 1.0,
        "Participation rate cannot exceed 1.0"
    );

    let last_vote = governance_vote::Entity::find()
        .filter(governance_vote::Column::Voter.eq(address.clone()))
        .order_by_desc(governance_vote::Column::VotedAt)
        .one(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let last_vote_at = last_vote.map(|vote| vote.voted_at.timestamp());

    let delegated_in =
        sum_delegations(governance_delegation::Column::Delegatee, &address, &state).await?;
    let delegated_out =
        sum_delegations(governance_delegation::Column::Delegator, &address, &state).await?;

    assert!(delegated_in >= 0, "Delegated inbound voting power negative");
    assert!(
        delegated_out >= 0,
        "Delegated outbound voting power negative"
    );

    let net = delegated_in - delegated_out;
    let net_voting_power = if net < 0 { 0 } else { net };

    let view = GovernanceStatsView {
        address,
        proposals_submitted,
        votes_cast,
        participation_rate,
        last_vote_at,
        delegated_in,
        delegated_out,
        net_voting_power,
    };

    Ok(Json(view))
}

async fn sum_delegations(
    column: governance_delegation::Column,
    address: &str,
    state: &AppState,
) -> Result<i64, HttpError> {
    let total = governance_delegation::Entity::find()
        .filter(column.eq(address.to_owned()))
        .select_only()
        .column_as(governance_delegation::Column::Amount.sum(), "total")
        .into_tuple::<Option<i64>>()
        .one(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .flatten()
        .unwrap_or(0);

    assert!(total >= 0, "Delegation aggregate must be non-negative");

    Ok(total)
}

fn count_to_i64(label: &str, count: u64) -> Result<i64, HttpError> {
    i64::try_from(count).map_err(|_| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{label} exceeds i64 bounds"),
        )
    })
}

fn parse_rfc3339_timestamp(label: &str, raw: &str) -> Result<i64, HttpError> {
    let parsed = DateTime::parse_from_rfc3339(raw).map_err(|err| {
        HttpError::new(
            StatusCode::BAD_GATEWAY,
            format!("Failed to parse {label} timestamp: {err}"),
        )
    })?;

    let timestamp = parsed.timestamp();
    assert!(timestamp >= 0, "Parsed timestamp must be non-negative");
    Ok(timestamp)
}

fn resolve_support_value(request: &VoteSubmissionRequest) -> Result<i32, HttpError> {
    if let Some(value) = request.support {
        if (0..=2).contains(&value) {
            return Ok(value);
        }

        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("Unsupported support value {value}"),
        ));
    }

    if let Some(option) = request.option.as_ref() {
        let normalized = option.trim().to_ascii_lowercase();
        return match normalized.as_str() {
            "yes" | "for" | "approve" => Ok(1),
            "no" | "against" | "reject" => Ok(0),
            "abstain" => Ok(2),
            "no_with_veto" | "veto" => Ok(0),
            _ => Err(HttpError::new(
                StatusCode::BAD_REQUEST,
                format!("Unsupported vote option {normalized}"),
            )),
        };
    }

    Err(HttpError::new(
        StatusCode::BAD_REQUEST,
        "support or option must be provided".to_string(),
    ))
}
