use std::sync::atomic::Ordering as AtomicOrdering;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::Method;
use axum::http::StatusCode;
use axum::http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::state::AppState;

mod governance;
mod identity;
mod privacy;

pub fn router(state: AppState) -> Router {
    assert!(
        state.start_time.elapsed() < Duration::from_secs(86_400),
        "Application uptime exceeds 24 hours before router creation"
    );

    // Configure CORS for web wallet access
    let cors = CorsLayer::new()
        // Allow requests from any origin (for development)
        // In production, restrict to specific domains
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([ACCEPT, AUTHORIZATION, CONTENT_TYPE])
        .max_age(Duration::from_secs(3600));

    let identity_router = identity::router().with_state(state.clone());
    let privacy_router = privacy::router().with_state(state.clone());
    let governance_router = governance::router().with_state(state.clone());
    Router::new()
        .route("/health", get(health_live))
        .route("/health/ready", get(health_ready))
        .nest("/identity", identity_router)
        .nest("/privacy", privacy_router)
        .nest("/governance", governance_router)
        .layer(cors)
        .with_state(state)
}

async fn health_live(State(state): State<AppState>) -> Result<Json<HealthResponse>, HttpError> {
    let uptime = state.start_time.elapsed().as_secs();
    assert!(
        uptime <= 31_536_000,
        "Uptime exceeds one year without restart"
    );
    let response = HealthResponse {
        status: "live",
        uptime_seconds: uptime,
    };
    Ok(Json(response))
}

async fn health_ready(State(state): State<AppState>) -> Result<Json<ReadyResponse>, HttpError> {
    state
        .database
        .ping()
        .await
        .map_err(|err| HttpError::new(StatusCode::SERVICE_UNAVAILABLE, err.to_string()))?;

    let last_block = state.last_indexed_block.load(AtomicOrdering::SeqCst);
    assert!(
        last_block <= i64::MAX as u64,
        "Last indexed block exceeds bounds"
    );
    assert!(
        last_block < 1_000_000_000_000,
        "Last indexed block sanity exceeded"
    );

    let rpc_timeout_ms =
        u64::try_from(state.rpc.timeout().as_millis()).expect("RPC timeout exceeds u64 bounds");

    let response = ReadyResponse {
        status: "ready",
        last_indexed_block: last_block,
        rpc_timeout_ms,
        cache_entries: CacheSummary {
            identity_profiles: state.cache.identity_profiles.entry_count(),
            identity_wallets: state.cache.identity_wallets.entry_count(),
            identity_search: state.cache.identity_search.entry_count(),
            leaderboards: state.cache.leaderboards.entry_count(),
            proposals: state.cache.proposals.entry_count(),
        },
    };
    Ok(Json(response))
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
struct ReadyResponse {
    status: &'static str,
    last_indexed_block: u64,
    rpc_timeout_ms: u64,
    cache_entries: CacheSummary,
}

#[derive(Debug, Serialize)]
struct CacheSummary {
    identity_profiles: u64,
    identity_wallets: u64,
    identity_search: u64,
    leaderboards: u64,
    proposals: u64,
}

#[derive(Debug)]
pub struct HttpError {
    status: StatusCode,
    message: String,
}

impl HttpError {
    pub fn new(status: StatusCode, message: String) -> Self {
        assert!(status != StatusCode::OK, "Error status cannot be 200");
        assert!(!message.is_empty(), "Error message cannot be empty");
        Self { status, message }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        info!("HTTP error: {}", self.message);
        let body = Json(ErrorBody {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}
