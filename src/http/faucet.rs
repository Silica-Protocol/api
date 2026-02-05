//! Faucet HTTP handlers for testnet token distribution.
//!
//! This module provides HTTP endpoints for the testnet faucet, including:
//! - Token drip requests with rate limiting
//! - Faucet status and balance queries
//! - Request history tracking
//!
//! # Security
//! - Rate limiting per address (24 hours)
//! - Rate limiting per IP (60 seconds)
//! - Optional CAPTCHA verification
//! - Request logging for abuse detection

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::info;

use crate::entities::faucet_request;
use crate::state::AppState;

use super::HttpError;

/// Maximum drip amount per request (100 CHERT)
pub const MAX_DRIP_AMOUNT: u64 = 100_000_000_000;

/// Default drip amount (10 CHERT)
pub const DEFAULT_DRIP_AMOUNT: u64 = 10_000_000_000;

/// Minimum drip amount (0.1 CHERT)
pub const MIN_DRIP_AMOUNT: u64 = 100_000_000;

/// Rate limit: one request per address every 24 hours
pub const ADDRESS_RATE_LIMIT_HOURS: i64 = 24;

/// Rate limit: one request per IP every 60 seconds
pub const IP_RATE_LIMIT_SECONDS: i64 = 60;

/// Maximum requests to return in history
pub const MAX_HISTORY_LIMIT: u64 = 100;

/// Faucet account address
pub const FAUCET_ADDRESS: &str = "faucet_0000000000000000000000000";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/drip", post(request_drip))
        .route("/status", get(get_status))
        .route("/history", get(get_history))
        .route("/check/:address", get(check_eligibility))
}

/// Request body for faucet drip
#[derive(Debug, Deserialize)]
pub struct FaucetDripRequest {
    /// Recipient wallet address
    pub address: String,
    /// Optional amount (defaults to DEFAULT_DRIP_AMOUNT)
    pub amount: Option<u64>,
    /// Optional CAPTCHA token for verification
    pub captcha_token: Option<String>,
}

/// Response from faucet drip
#[derive(Debug, Serialize)]
pub struct FaucetDripResponse {
    pub success: bool,
    pub tx_hash: String,
    pub amount: u64,
    pub amount_formatted: String,
    pub recipient: String,
    pub message: String,
    pub next_eligible_at: Option<DateTime<Utc>>,
}

/// Faucet status response
#[derive(Debug, Serialize)]
pub struct FaucetStatusResponse {
    pub faucet_address: String,
    pub balance: u64,
    pub balance_formatted: String,
    pub default_drip: u64,
    pub default_drip_formatted: String,
    pub max_drip: u64,
    pub min_drip: u64,
    pub drips_available: u64,
    pub rate_limit_hours: i64,
    pub status: String,
    pub total_distributed: u64,
    pub total_requests: u64,
}

/// Eligibility check response
#[derive(Debug, Serialize)]
pub struct EligibilityResponse {
    pub address: String,
    pub eligible: bool,
    pub next_eligible_at: Option<DateTime<Utc>>,
    pub wait_seconds: Option<i64>,
    pub message: String,
}

/// Faucet history entry
#[derive(Debug, Serialize)]
pub struct FaucetHistoryEntry {
    pub tx_hash: String,
    pub recipient: String,
    pub amount: u64,
    pub amount_formatted: String,
    pub created_at: DateTime<Utc>,
}

/// History query parameters
#[derive(Debug, Deserialize, Default)]
pub struct HistoryQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub address: Option<String>,
}

/// Request tokens from the faucet
async fn request_drip(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<FaucetDripRequest>,
) -> Result<Json<FaucetDripResponse>, HttpError> {
    let ip_address = addr.ip().to_string();

    // Validate address format
    if request.address.is_empty() || request.address.len() < 32 || request.address.len() > 64 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "Invalid wallet address format".to_string(),
        ));
    }

    // Validate amount
    let amount = request.amount.unwrap_or(DEFAULT_DRIP_AMOUNT);
    if amount < MIN_DRIP_AMOUNT {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("Amount below minimum of {} base units", MIN_DRIP_AMOUNT),
        ));
    }
    if amount > MAX_DRIP_AMOUNT {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("Amount exceeds maximum of {} base units", MAX_DRIP_AMOUNT),
        ));
    }

    // Check address rate limit
    let address_cutoff = Utc::now() - Duration::hours(ADDRESS_RATE_LIMIT_HOURS);
    let recent_by_address = faucet_request::Entity::find()
        .filter(faucet_request::Column::RecipientAddress.eq(&request.address))
        .filter(faucet_request::Column::CreatedAt.gt(address_cutoff))
        .order_by_desc(faucet_request::Column::CreatedAt)
        .one(&state.database)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(last_request) = recent_by_address {
        let next_eligible = last_request.created_at.with_timezone(&Utc) + Duration::hours(ADDRESS_RATE_LIMIT_HOURS);
        let wait_seconds = (next_eligible - Utc::now()).num_seconds();
        if wait_seconds > 0 {
            return Err(HttpError::new(
                StatusCode::TOO_MANY_REQUESTS,
                format!(
                    "Rate limited. Please wait {} hours before requesting again.",
                    (wait_seconds / 3600) + 1
                ),
            ));
        }
    }

    // Check IP rate limit
    let ip_cutoff = Utc::now() - Duration::seconds(IP_RATE_LIMIT_SECONDS);
    let recent_by_ip = faucet_request::Entity::find()
        .filter(faucet_request::Column::IpAddress.eq(&ip_address))
        .filter(faucet_request::Column::CreatedAt.gt(ip_cutoff))
        .one(&state.database)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if recent_by_ip.is_some() {
        return Err(HttpError::new(
            StatusCode::TOO_MANY_REQUESTS,
            format!("Please wait {} seconds between requests from the same IP.", IP_RATE_LIMIT_SECONDS),
        ));
    }

    // TODO: Verify CAPTCHA if provided
    // if let Some(token) = request.captcha_token {
    //     verify_captcha(&token).await?;
    // }

    // Call the node RPC to perform the drip
    let drip_result = state
        .rpc
        .faucet_drip(&request.address, amount)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Record the request in the database
    let now_fixed = Utc::now().fixed_offset();
    let new_request = faucet_request::ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        recipient_address: sea_orm::ActiveValue::Set(request.address.clone()),
        ip_address: sea_orm::ActiveValue::Set(ip_address),
        amount: sea_orm::ActiveValue::Set(amount as i64),
        tx_hash: sea_orm::ActiveValue::Set(drip_result.tx_hash.clone()),
        created_at: sea_orm::ActiveValue::Set(now_fixed),
    };

    faucet_request::Entity::insert(new_request)
        .exec(&state.database)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!(
        "Faucet drip: {} tokens to {} (tx: {})",
        amount, request.address, drip_result.tx_hash
    );

    let next_eligible_at = Utc::now() + Duration::hours(ADDRESS_RATE_LIMIT_HOURS);

    Ok(Json(FaucetDripResponse {
        success: true,
        tx_hash: drip_result.tx_hash,
        amount,
        amount_formatted: format_balance(amount),
        recipient: request.address,
        message: "Tokens sent! They should arrive within a few seconds.".to_string(),
        next_eligible_at: Some(next_eligible_at),
    }))
}

/// Get faucet status
async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<FaucetStatusResponse>, HttpError> {
    // Get faucet balance from node
    let faucet_status = state
        .rpc
        .faucet_status()
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get total distributed from database
    let stats = faucet_request::Entity::find()
        .select_only()
        .column_as(faucet_request::Column::Amount.sum(), "total_amount")
        .column_as(faucet_request::Column::Id.count(), "total_count")
        .into_tuple::<(Option<i64>, i64)>()
        .one(&state.database)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or((None, 0));

    let total_distributed = stats.0.unwrap_or(0) as u64;
    let total_requests = stats.1 as u64;

    Ok(Json(FaucetStatusResponse {
        faucet_address: faucet_status.faucet_address,
        balance: faucet_status.balance,
        balance_formatted: format_balance(faucet_status.balance),
        default_drip: DEFAULT_DRIP_AMOUNT,
        default_drip_formatted: format_balance(DEFAULT_DRIP_AMOUNT),
        max_drip: MAX_DRIP_AMOUNT,
        min_drip: MIN_DRIP_AMOUNT,
        drips_available: faucet_status.drips_available,
        rate_limit_hours: ADDRESS_RATE_LIMIT_HOURS,
        status: faucet_status.status,
        total_distributed,
        total_requests,
    }))
}

/// Check if an address is eligible for a drip
async fn check_eligibility(
    State(state): State<AppState>,
    axum::extract::Path(address): axum::extract::Path<String>,
) -> Result<Json<EligibilityResponse>, HttpError> {
    // Validate address format
    if address.is_empty() || address.len() < 32 || address.len() > 64 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "Invalid wallet address format".to_string(),
        ));
    }

    // Check for recent requests
    let cutoff = Utc::now() - Duration::hours(ADDRESS_RATE_LIMIT_HOURS);
    let recent_request = faucet_request::Entity::find()
        .filter(faucet_request::Column::RecipientAddress.eq(&address))
        .filter(faucet_request::Column::CreatedAt.gt(cutoff))
        .order_by_desc(faucet_request::Column::CreatedAt)
        .one(&state.database)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match recent_request {
        Some(last_request) => {
            let next_eligible = last_request.created_at.with_timezone(&Utc) + Duration::hours(ADDRESS_RATE_LIMIT_HOURS);
            let wait_seconds = (next_eligible - Utc::now()).num_seconds();
            
            if wait_seconds > 0 {
                let hours = wait_seconds / 3600;
                let minutes = (wait_seconds % 3600) / 60;
                Ok(Json(EligibilityResponse {
                    address,
                    eligible: false,
                    next_eligible_at: Some(next_eligible),
                    wait_seconds: Some(wait_seconds),
                    message: format!("Please wait {}h {}m before requesting again", hours, minutes),
                }))
            } else {
                Ok(Json(EligibilityResponse {
                    address,
                    eligible: true,
                    next_eligible_at: None,
                    wait_seconds: None,
                    message: "You are eligible to request tokens".to_string(),
                }))
            }
        }
        None => {
            Ok(Json(EligibilityResponse {
                address,
                eligible: true,
                next_eligible_at: None,
                wait_seconds: None,
                message: "You are eligible to request tokens".to_string(),
            }))
        }
    }
}

/// Get faucet request history
async fn get_history(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> Result<Json<Vec<FaucetHistoryEntry>>, HttpError> {
    let limit = query.limit.unwrap_or(20).min(MAX_HISTORY_LIMIT);
    let offset = query.offset.unwrap_or(0);

    let mut select = faucet_request::Entity::find();

    // Filter by address if provided
    if let Some(address) = query.address {
        select = select.filter(faucet_request::Column::RecipientAddress.eq(address));
    }

    let requests = select
        .order_by_desc(faucet_request::Column::CreatedAt)
        .limit(limit)
        .offset(offset)
        .all(&state.database)
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<FaucetHistoryEntry> = requests
        .into_iter()
        .map(|r| FaucetHistoryEntry {
            tx_hash: r.tx_hash,
            recipient: r.recipient_address,
            amount: r.amount as u64,
            amount_formatted: format_balance(r.amount as u64),
            created_at: r.created_at.with_timezone(&Utc),
        })
        .collect();

    Ok(Json(entries))
}

/// Format a balance in base units to a human-readable string
fn format_balance(base_units: u64) -> String {
    let whole = base_units / 1_000_000_000;
    let frac = base_units % 1_000_000_000;
    if frac == 0 {
        format!("{} CHERT", whole)
    } else {
        // Trim trailing zeros
        let frac_str = format!("{:09}", frac);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{} CHERT", whole, trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_balance() {
        assert_eq!(format_balance(0), "0 CHERT");
        assert_eq!(format_balance(1_000_000_000), "1 CHERT");
        assert_eq!(format_balance(10_000_000_000), "10 CHERT");
        assert_eq!(format_balance(100_000_000_000), "100 CHERT");
        assert_eq!(format_balance(1_500_000_000), "1.5 CHERT");
        assert_eq!(format_balance(123_456_789), "0.123456789 CHERT");
        assert_eq!(format_balance(100_000_000), "0.1 CHERT");
    }

    #[test]
    fn test_amount_bounds() {
        assert!(MIN_DRIP_AMOUNT < DEFAULT_DRIP_AMOUNT);
        assert!(DEFAULT_DRIP_AMOUNT < MAX_DRIP_AMOUNT);
    }
}
