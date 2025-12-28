use std::sync::atomic::Ordering as AtomicOrdering;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use silica::privacy::{SpendPublicKey, StealthKeyPair, ViewPublicKey};
use silica_models::stealth::STEALTH_OUTPUT_MEMO_MAX_BYTES;
use tracing::error;

use crate::models::privacy::{
    StealthAddressRequestPayload, StealthAddressResponsePayload, StealthKeyBundlePayload,
    StealthScanRangeSummary, StealthScanRequestPayload, StealthScanResponsePayload,
    StealthTransferRequestPayload, StealthTransferResponsePayload,
};
use crate::state::AppState;
use crate::stealth_scanner::{ScanError, scan_owned_outputs};

use super::HttpError;

const SEED_HEX_BYTES: usize = 32;
const MAX_STEALTH_SCAN_RESULTS: u64 = 1_024;
const MAX_STEALTH_SCAN_BLOCK_RANGE: u64 = 10_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stealth/address", post(generate_address))
        .route("/stealth/scan", post(scan_outputs))
        .route("/stealth/transfer", post(submit_transfer))
}

async fn generate_address(
    State(state): State<AppState>,
    Json(mut payload): Json<StealthAddressRequestPayload>,
) -> Result<Json<StealthAddressResponsePayload>, HttpError> {
    if let Some(seed) = payload.seed_hex.as_mut() {
        let normalized = seed.trim();
        validate_hex_length(normalized, SEED_HEX_BYTES, "seed_hex")?;
        *seed = normalized.to_lowercase();
    }

    let response = state
        .rpc
        .generate_stealth_address(&payload)
        .await
        .map_err(|err| HttpError::new(StatusCode::BAD_GATEWAY, err.to_string()))?;

    Ok(Json(response))
}

async fn scan_outputs(
    State(state): State<AppState>,
    Json(payload): Json<StealthScanRequestPayload>,
) -> Result<Json<StealthScanResponsePayload>, HttpError> {
    let keys = parse_stealth_keypair(&payload.stealth_keys)?;

    let limit = payload.limit.unwrap_or(MAX_STEALTH_SCAN_RESULTS);
    if limit == 0 || limit > MAX_STEALTH_SCAN_RESULTS {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_STEALTH_SCAN_RESULTS}"),
        ));
    }

    let latest_block = state.last_indexed_block.load(AtomicOrdering::SeqCst);
    let from_block = payload.from_block.unwrap_or(0);
    if from_block > latest_block {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("from_block {from_block} exceeds latest indexed block {latest_block}"),
        ));
    }

    let mut to_block = payload.to_block.unwrap_or(latest_block);
    if to_block > latest_block {
        to_block = latest_block;
    }

    if to_block < from_block {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "to_block must be greater than or equal to from_block".to_string(),
        ));
    }

    let span = to_block.saturating_sub(from_block);
    if span > MAX_STEALTH_SCAN_BLOCK_RANGE {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "Requested scan range {span} exceeds static limit of {MAX_STEALTH_SCAN_BLOCK_RANGE} blocks",
            ),
        ));
    }

    let limit_usize = usize::try_from(limit).map_err(|_| {
        HttpError::new(
            StatusCode::BAD_REQUEST,
            "limit exceeds platform bounds".to_string(),
        )
    })?;

    let outcome = scan_owned_outputs(&state.database, &keys, from_block, to_block, limit_usize)
        .await
        .map_err(map_scan_error)?;

    let total_scanned = u64::try_from(outcome.total_scanned).map_err(|_| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Scanned output count exceeds u64 bounds".to_string(),
        )
    })?;

    let response = StealthScanResponsePayload {
        range: StealthScanRangeSummary {
            from_block,
            to_block,
            span,
        },
        latest_block,
        total_scanned,
        total_balance: outcome.total_balance,
        transactions_returned: outcome.transactions.len(),
        has_more: outcome.has_more,
        transactions: outcome.transactions,
    };

    Ok(Json(response))
}

fn map_scan_error(err: ScanError) -> HttpError {
    match err {
        ScanError::Database(source) => {
            error!(?source, "Stealth scan database error");
            HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to query stealth outputs".to_string(),
            )
        }
        ScanError::BlockBoundExceeded { block } => HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("Block number {block} exceeds storage bounds"),
        ),
        ScanError::OutputOverflow { observed, limit } => HttpError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "Requested scan returned {observed} outputs which exceeds the defensive bound of {limit}"
            ),
        ),
    }
}

async fn submit_transfer(
    State(state): State<AppState>,
    Json(payload): Json<StealthTransferRequestPayload>,
) -> Result<Json<StealthTransferResponsePayload>, HttpError> {
    if payload.amount == 0 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "amount must be greater than zero".to_string(),
        ));
    }

    if let Some(ref memo) = payload.memo {
        if memo.len() > STEALTH_OUTPUT_MEMO_MAX_BYTES {
            return Err(HttpError::new(
                StatusCode::BAD_REQUEST,
                format!("memo length must not exceed {STEALTH_OUTPUT_MEMO_MAX_BYTES} bytes",),
            ));
        }
    }

    validate_stealth_keys(&payload.sender_keys)?;
    let view_key = parse_view_key(&payload.recipient_view_key)?;
    let spend_key = parse_spend_key(&payload.recipient_spend_key)?;

    let response = state
        .rpc
        .submit_stealth_transfer(&payload, &view_key, &spend_key)
        .await
        .map_err(|err| HttpError::new(StatusCode::BAD_GATEWAY, err.to_string()))?;

    Ok(Json(response))
}

fn validate_hex_length(value: &str, expected_bytes: usize, field: &str) -> Result<(), HttpError> {
    if value.len() != expected_bytes * 2 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "{field} must be {} hex characters ({} bytes)",
                expected_bytes * 2,
                expected_bytes
            ),
        ));
    }

    if hex::decode(value).is_err() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            format!("{field} must be valid lowercase hex"),
        ));
    }

    Ok(())
}

fn validate_stealth_keys(keys: &StealthKeyBundlePayload) -> Result<(), HttpError> {
    parse_stealth_keypair(keys).map(|_| ())
}

fn parse_stealth_keypair(keys: &StealthKeyBundlePayload) -> Result<StealthKeyPair, HttpError> {
    StealthKeyPair::from_hex_components(
        &keys.view_keypair.public,
        &keys.view_keypair.secret,
        &keys.spend_keypair.public,
        &keys.spend_keypair.secret,
    )
    .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))
}

fn parse_view_key(value: &str) -> Result<ViewPublicKey, HttpError> {
    ViewPublicKey::from_hex(value)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))
}

fn parse_spend_key(value: &str) -> Result<SpendPublicKey, HttpError> {
    SpendPublicKey::from_hex(value)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))
}
