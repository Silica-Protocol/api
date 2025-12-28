mod config;
mod entities;
mod http;
mod identity;
mod indexer;
mod models;
mod rpc;
mod state;
mod stealth_scanner;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use crate::config::ApiConfig;
use crate::indexer::ChainIndexer;
use crate::rpc::RpcClient;
use crate::state::{ApiCache, AppState};
use anyhow::{Context, Result};
use axum::Router;
use migration::MigratorTrait;
use sea_orm::ConnectOptions;
use sea_orm::Database;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = ApiConfig::load().context("Failed to load configuration")?;
    let database = connect_database(&config).await?;
    run_migrations(&database).await?;

    let rpc_client = RpcClient::new(&config.chain.rpc_url, config.chain.request_timeout())
        .context("Failed to initialize RPC client")?;

    let cache = Arc::new(ApiCache::new(&config.cache));
    let last_indexed_block = Arc::new(AtomicU64::new(0));
    let app_state = AppState::new(
        database.clone(),
        Arc::clone(&cache),
        rpc_client.clone(),
        Arc::clone(&last_indexed_block),
    );

    let indexer = ChainIndexer::new(
        database.clone(),
        rpc_client.clone(),
        config.indexer.clone(),
        Arc::clone(&last_indexed_block),
        Arc::clone(&cache),
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let indexer_handle = tokio::spawn(async move {
        if let Err(err) = indexer.run(shutdown_rx).await {
            error!("Indexer terminated with error: {err}");
        }
    });

    let listener = TcpListener::bind(config.server.address())
        .await
        .context("Failed to bind HTTP listener")?;
    let local_addr = listener
        .local_addr()
        .context("Failed to obtain listener address")?;
    info!("Chert API listening on {local_addr}");

    let router: Router = http::router(app_state.clone());
    let server = axum::serve(listener, router.into_make_service());
    server
        .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()))
        .await
        .context("HTTP server exited with error")?;

    shutdown_tx.send(true).ok();
    if let Err(join_err) = indexer_handle.await {
        error!("Indexer task join error: {join_err}");
    }

    Ok(())
}

fn init_tracing() {
    let default_filter = "info";
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| default_filter.to_string());
    assert!(!filter.is_empty(), "Tracing filter must not be empty");
    assert!(filter.len() < 256, "Tracing filter length exceeds bounds");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .compact()
        .init();
}

async fn connect_database(config: &ApiConfig) -> Result<sea_orm::DatabaseConnection> {
    let mut options = ConnectOptions::new(config.database.url.clone());
    options
        .max_connections(config.database.max_connections)
        .sqlx_logging(true)
        .sqlx_logging_level(tracing::log::LevelFilter::Debug)
        .acquire_timeout(Duration::from_secs(10));

    if let Some(min) = config.database.min_connections {
        options.min_connections(min);
    }

    assert!(
        config.database.max_connections >= config.database.min_connections.unwrap_or(1),
        "Max connections must be >= min connections"
    );
    assert!(
        config.database.max_connections <= 128,
        "Connection pool oversized"
    );

    Database::connect(options)
        .await
        .context("Failed to connect to PostgreSQL")
}

async fn run_migrations(database: &sea_orm::DatabaseConnection) -> Result<()> {
    migration::Migrator::up(database, None)
        .await
        .context("Database migrations failed")
}

async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    if let Err(err) = tokio::signal::ctrl_c().await {
        error!("Failed to listen for shutdown signal: {err}");
        return;
    }
    shutdown_tx.send(true).ok();
    info!("Shutdown signal dispatched");
}
