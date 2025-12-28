use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use moka::future::Cache;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::config::CacheConfig;
use crate::models::identity::{IdentityProfileView, IdentitySearchResult, WalletLinkView};
use crate::rpc::RpcClient;

#[derive(Clone)]
pub struct AppState {
    pub database: DatabaseConnection,
    pub cache: Arc<ApiCache>,
    pub rpc: RpcClient,
    pub start_time: Instant,
    pub last_indexed_block: Arc<AtomicU64>,
}

impl AppState {
    pub fn new(
        database: DatabaseConnection,
        cache: Arc<ApiCache>,
        rpc: RpcClient,
        last_indexed_block: Arc<AtomicU64>,
    ) -> Self {
        assert!(
            cache.identity_capacity >= 100,
            "Identity cache capacity must be configured"
        );
        assert!(
            Arc::strong_count(&last_indexed_block) >= 1,
            "Indexer state must be shared"
        );
        Self {
            database,
            cache,
            rpc,
            start_time: Instant::now(),
            last_indexed_block,
        }
    }
}

pub struct ApiCache {
    pub identity_profiles: Cache<String, Arc<IdentityProfileView>>,
    pub identity_wallets: Cache<String, Arc<Vec<WalletLinkView>>>,
    pub identity_search: Cache<String, Arc<Vec<IdentitySearchResult>>>,
    pub leaderboards: Cache<String, Value>,
    pub proposals: Cache<String, Value>,
    pub identity_capacity: u64,
}

impl ApiCache {
    pub fn new(config: &CacheConfig) -> Self {
        assert!(
            config.identities_max_capacity >= 100,
            "Identity cache capacity threshold"
        );
        assert!(
            config.leaderboards_max_capacity >= 10,
            "Leaderboard cache capacity threshold"
        );

        let identity_profiles = Cache::builder()
            .max_capacity(config.identities_max_capacity)
            .time_to_live(Duration::from_secs(config.identities_ttl_seconds))
            .time_to_idle(Duration::from_secs(config.identities_ttl_seconds / 2 + 1))
            .build();

        let identity_wallets = Cache::builder()
            .max_capacity(config.identities_max_capacity)
            .time_to_live(Duration::from_secs(config.identities_ttl_seconds))
            .time_to_idle(Duration::from_secs(config.identities_ttl_seconds / 2 + 1))
            .build();

        let identity_search = Cache::builder()
            .max_capacity(config.identities_max_capacity)
            .time_to_live(Duration::from_secs(config.identities_ttl_seconds))
            .time_to_idle(Duration::from_secs(config.identities_ttl_seconds / 2 + 1))
            .build();

        let leaderboards = Cache::builder()
            .max_capacity(config.leaderboards_max_capacity)
            .time_to_live(Duration::from_secs(config.leaderboards_ttl_seconds))
            .time_to_idle(Duration::from_secs(config.leaderboards_ttl_seconds / 2 + 1))
            .build();

        let proposals = Cache::builder()
            .max_capacity(config.proposals_max_capacity)
            .time_to_live(Duration::from_secs(config.proposals_ttl_seconds))
            .time_to_idle(Duration::from_secs(config.proposals_ttl_seconds / 2 + 1))
            .build();

        Self {
            identity_profiles,
            identity_wallets,
            identity_search,
            leaderboards,
            proposals,
            identity_capacity: config.identities_max_capacity,
        }
    }
}
