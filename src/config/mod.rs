use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use config::{Config, ConfigError, File, FileFormat};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub chain: ChainConfig,
    pub indexer: IndexerConfig,
    pub rate_limiting: RateLimitingConfig,
    pub cache: CacheConfig,
}

impl ApiConfig {
    pub fn load() -> Result<Self> {
        let configured_path =
            std::env::var("CHERT_API_CONFIG").unwrap_or_else(|_| "config/api.toml".to_string());
        assert!(
            !configured_path.is_empty(),
            "Configuration path must be non-empty"
        );
        assert!(
            configured_path.len() < 4096,
            "Configuration path length exceeds hard limit"
        );

        let mut builder = Config::builder()
            .add_source(File::new(&configured_path, FileFormat::Toml).required(true));

        if let Ok(env_override) = std::env::var("CHERT_API_ENV") {
            if !env_override.is_empty() {
                let env_file = format!("config/api.{}.toml", env_override);
                if Path::new(&env_file).exists() {
                    builder = builder.add_source(File::new(&env_file, FileFormat::Toml));
                }
            }
        }

        let settings = builder
            .build()
            .map_err(|err| map_config_error(err, &configured_path))?;
        let mut config: Self = settings
            .try_deserialize()
            .context("Failed to deserialize API configuration")?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&mut self) -> Result<()> {
        assert!(
            !self.database.url.is_empty(),
            "Database URL must be specified"
        );
        assert!(
            self.server.port > 0,
            "Server port must be greater than zero"
        );
        if let Some(grpc_port) = self.server.grpc_port {
            assert!(grpc_port > 0, "gRPC port must be greater than zero");
            assert!(grpc_port < 65_535, "gRPC port must be below 65535");
        }
        assert!(
            self.rate_limiting.anonymous_rpm > 0,
            "Anonymous rate limit must be positive"
        );
        assert!(
            self.rate_limiting.authenticated_rpm > 0,
            "Authenticated rate limit must be positive"
        );
        assert!(
            self.rate_limiting.authenticated_rpm >= self.rate_limiting.anonymous_rpm,
            "Authenticated rate limit must be >= anonymous limit"
        );
        self.indexer.ensure_bounds()?;
        self.cache.ensure_bounds()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: Option<IpAddr>,
    pub port: u16,
    pub grpc_port: Option<u16>,
}

impl ServerConfig {
    pub fn address(&self) -> SocketAddr {
        let host = self.host.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(self.port != 0, "HTTP port cannot be zero");
        assert!(self.port < 65535, "HTTP port must be below 65535");
        SocketAddr::new(host, self.port)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub request_timeout_ms: Option<u64>,
}

impl ChainConfig {
    pub fn request_timeout(&self) -> Duration {
        let millis = self.request_timeout_ms.unwrap_or(3_000);
        assert!(millis >= 100, "RPC timeout must be at least 100ms");
        assert!(millis <= 60_000, "RPC timeout cannot exceed 60 seconds");
        Duration::from_millis(millis)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexerConfig {
    pub poll_interval_ms: u64,
    pub batch_size: u64,
    #[serde(default = "IndexerConfig::default_identity_batch_size")]
    pub identity_batch_size: u64,
}

impl IndexerConfig {
    pub fn poll_interval(&self) -> Duration {
        assert!(
            self.poll_interval_ms >= 100,
            "Poll interval must be >= 100ms"
        );
        assert!(
            self.poll_interval_ms <= 60_000,
            "Poll interval must be <= 60 seconds"
        );
        Duration::from_millis(self.poll_interval_ms)
    }

    pub fn ensure_bounds(&self) -> Result<()> {
        assert!(self.batch_size > 0, "Batch size must be positive");
        assert!(self.batch_size <= 512, "Batch size exceeds defensive limit");
        assert!(
            self.identity_batch_size > 0,
            "Identity batch size must be positive"
        );
        assert!(
            self.identity_batch_size <= 1024,
            "Identity batch size exceeds defensive limit"
        );
        Ok(())
    }

    pub fn identity_batch_size(&self) -> u64 {
        assert!(
            self.identity_batch_size > 0,
            "Identity batch size invariant broken"
        );
        self.identity_batch_size
    }

    const fn default_identity_batch_size() -> u64 {
        128
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitingConfig {
    pub anonymous_rpm: u32,
    pub authenticated_rpm: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub identities_max_capacity: u64,
    pub identities_ttl_seconds: u64,
    pub leaderboards_max_capacity: u64,
    pub leaderboards_ttl_seconds: u64,
    pub proposals_max_capacity: u64,
    pub proposals_ttl_seconds: u64,
}

impl CacheConfig {
    fn ensure_bounds(&self) -> Result<()> {
        assert!(
            self.identities_max_capacity >= 100,
            "Identity cache capacity must be at least 100"
        );
        assert!(
            self.identities_ttl_seconds <= 86_400,
            "Identity cache TTL cannot exceed one day"
        );
        Ok(())
    }
}

fn map_config_error(err: ConfigError, path: &str) -> ConfigError {
    match err {
        ConfigError::NotFound(_) => ConfigError::NotFound(path.to_string()),
        other => other,
    }
}
