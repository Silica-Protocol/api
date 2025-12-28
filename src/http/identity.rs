use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};

use crate::entities::{identity_profile, wallet_link};
use crate::identity::{
    decode_identity_id, decode_signature, encode_identity_id, sanitize_wallet_address,
};
use crate::models::identity::{IdentityProfileView, IdentitySearchResult, WalletLinkView};
use crate::state::AppState;

use super::HttpError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search", get(search_profiles))
        .route("/:identity_id", get(get_profile))
        .route("/:identity_id/wallets", get(get_wallets))
        .route("/:identity_id/wallets/verify", post(verify_wallet_link))
}

async fn get_profile(
    Path(identity_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<IdentityProfileView>, HttpError> {
    let identity_bytes = decode_identity_id(&identity_id)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let canonical_id = encode_identity_id(&identity_bytes);

    if let Some(cached) = state.cache.identity_profiles.get(&canonical_id).await {
        return Ok(Json((*cached).clone()));
    }

    let profile = identity_profile::Entity::find_by_id(identity_bytes.clone())
        .one(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                format!("Identity {identity_id} not found"),
            )
        })?;

    let wallet_count = wallet_link::Entity::find()
        .filter(wallet_link::Column::IdentityId.eq(identity_bytes.clone()))
        .count(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    assert!(
        wallet_count <= u32::MAX as u64,
        "Wallet count exceeds u32 bounds"
    );

    let view = IdentityProfileView {
        identity_id: canonical_id.clone(),
        display_name: profile.display_name.clone(),
        avatar_hash: profile.avatar_hash.as_ref().map(hex::encode),
        bio: profile.bio.clone(),
        stats_visibility: profile.stats_visibility.clone(),
        wallet_count: wallet_count as u32,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
        last_synced_block: profile.last_synced_block,
        profile_version: profile.profile_version,
    };

    state
        .cache
        .identity_profiles
        .insert(canonical_id.clone(), Arc::new(view.clone()))
        .await;

    Ok(Json(view))
}

async fn get_wallets(
    Path(identity_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<WalletLinkView>>, HttpError> {
    let identity_bytes = decode_identity_id(&identity_id)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let canonical_id = encode_identity_id(&identity_bytes);

    if let Some(cached) = state.cache.identity_wallets.get(&canonical_id).await {
        return Ok(Json((*cached).clone()));
    }

    let links = wallet_link::Entity::find()
        .filter(wallet_link::Column::IdentityId.eq(identity_bytes.clone()))
        .order_by_desc(wallet_link::Column::VerifiedAt)
        .order_by_desc(wallet_link::Column::CreatedAt)
        .all(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mut views = Vec::with_capacity(links.len());
    for (index, link) in links.iter().enumerate() {
        assert!(
            index < crate::identity::MAX_WALLET_LINKS,
            "Wallet link bound exceeded"
        );
        views.push(WalletLinkView {
            wallet_address: link.wallet_address.clone(),
            link_type: link.link_type.clone(),
            proof_signature: hex::encode(&link.proof_signature),
            created_at: link.created_at,
            verified_at: link.verified_at,
            last_synced_block: link.last_synced_block,
        });
    }

    let arc_views = Arc::new(views.clone());
    state
        .cache
        .identity_wallets
        .insert(canonical_id.clone(), arc_views)
        .await;

    Ok(Json(views))
}

async fn verify_wallet_link(
    Path(identity_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<WalletVerificationRequest>,
) -> Result<Json<WalletVerificationResponse>, HttpError> {
    let identity_bytes = decode_identity_id(&identity_id)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let canonical_id = encode_identity_id(&identity_bytes);
    let sanitized_address = sanitize_wallet_address(&payload.wallet_address)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;

    let link = wallet_link::Entity::find()
        .filter(wallet_link::Column::IdentityId.eq(identity_bytes))
        .filter(wallet_link::Column::WalletAddress.eq(sanitized_address.clone()))
        .one(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    if let Some(link) = link {
        let stored_signature = link.proof_signature.clone();
        let provided_matches = if let Some(signature) = payload.signature.as_deref() {
            let provided = decode_signature(signature)
                .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
            provided == stored_signature
        } else {
            link.verified_at.is_some()
        };

        let response = WalletVerificationResponse {
            identity_id: canonical_id,
            wallet_address: sanitized_address,
            linked: true,
            verified: provided_matches,
            proof_signature: Some(hex::encode(stored_signature)),
            verified_at: link.verified_at,
            last_synced_block: Some(link.last_synced_block),
            reason: if provided_matches {
                None
            } else {
                Some("Signature mismatch or verification pending".to_string())
            },
        };
        return Ok(Json(response));
    }

    let response = WalletVerificationResponse {
        identity_id: canonical_id,
        wallet_address: sanitized_address,
        linked: false,
        verified: false,
        proof_signature: None,
        verified_at: None,
        last_synced_block: None,
        reason: Some("Wallet not linked to identity".to_string()),
    };
    Ok(Json(response))
}

async fn search_profiles(
    Query(params): Query<IdentitySearchParams>,
    State(state): State<AppState>,
) -> Result<Json<IdentitySearchResponse>, HttpError> {
    let query = params.q.trim();
    if query.is_empty() {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "Query parameter 'q' must not be empty".to_string(),
        ));
    }
    assert!(
        query.len() <= crate::identity::MAX_DISPLAY_NAME_LEN,
        "Query too long"
    );

    let normalized = query.to_ascii_lowercase();
    if normalized.len() < 2 {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "Query must be at least two characters".to_string(),
        ));
    }

    let limit = params.limit.unwrap_or(20);
    assert!(limit > 0, "Search limit must be positive");
    assert!(limit <= 100, "Search limit exceeds defensive bound");

    let cache_key = format!("{}::{limit}", normalized);
    if let Some(cached) = state.cache.identity_search.get(&cache_key).await {
        let response = IdentitySearchResponse {
            query: normalized.clone(),
            limit,
            results: (*cached).clone(),
        };
        return Ok(Json(response));
    }

    let profiles = identity_profile::Entity::find()
        .filter(identity_profile::Column::DisplayNameSearch.contains(&normalized))
        .order_by_desc(identity_profile::Column::UpdatedAt)
        .limit(u64::from(limit))
        .all(&state.database)
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mut results = Vec::with_capacity(profiles.len());
    for model in &profiles {
        results.push(IdentitySearchResult {
            identity_id: encode_identity_id(&model.identity_id),
            display_name: model.display_name.clone(),
            stats_visibility: model.stats_visibility.clone(),
            updated_at: model.updated_at,
        });
    }

    let arc_results = Arc::new(results.clone());
    state
        .cache
        .identity_search
        .insert(cache_key, arc_results)
        .await;

    let response = IdentitySearchResponse {
        query: normalized,
        limit,
        results,
    };
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
struct IdentitySearchParams {
    q: String,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
struct IdentitySearchResponse {
    query: String,
    limit: u32,
    results: Vec<IdentitySearchResult>,
}

#[derive(Debug, Deserialize)]
struct WalletVerificationRequest {
    wallet_address: String,
    signature: Option<String>,
}

#[derive(Debug, Serialize)]
struct WalletVerificationResponse {
    identity_id: String,
    wallet_address: String,
    linked: bool,
    verified: bool,
    proof_signature: Option<String>,
    verified_at: Option<i64>,
    last_synced_block: Option<i64>,
    reason: Option<String>,
}
