# Chert API Backend

**Status**: 🔄 **PLANNED** - Aggregation and query layer for Chert ecosystem  
**Purpose**: REST/gRPC API for wallets, explorers, governance tools, and social features

---

## 🎯 Overview

The Chert API backend serves as the **aggregation and query layer** sitting between the blockchain/oracle infrastructure and end-user applications. It provides:

- **Fast queries** - Pre-computed stats and indexed data for sub-100ms responses
- **Cross-chain aggregation** - Combines on-chain data with oracle work submissions
- **Rich analytics** - PoUW leaderboards, governance metrics, user profiles
- **External integrations** - Public API for explorers, wallets, and third-party tools

**Architecture Philosophy**:
- **Read-only by design** - API never holds private keys or submits transactions
- **Stateless compute** - All source data lives on-chain or in oracles
- **Horizontally scalable** - Multiple instances can run against same data
- **Optional service** - Wallets can query blockchain directly; API adds convenience

---

## 🏗️ System Architecture

```
┌──────────────────────────────────────────────────────┐
│             Chert Blockchain Network                 │
│  • On-chain transactions, balances, votes           │
│  • Identity registry, public profiles               │
│  • Governance proposals and voting records          │
└────────────────────┬─────────────────────────────────┘
                     │
                     │ Reads via RPC
                     │
┌────────────────────▼─────────────────────────────────┐
│             Oracle Services Layer                    │
│  • BOINC work validation records                    │
│  • NUW task submissions and results                 │
│  • Oracle performance metrics                       │
└────────────────────┬─────────────────────────────────┘
                     │
                     │ Indexes continuously
                     │
┌────────────────────▼─────────────────────────────────┐
│             Chert API Backend (THIS)                 │
│                                                       │
│  ┌─────────────────────────────────────────┐        │
│  │     Chain Indexer (Background Task)     │        │
│  │  • Syncs blocks, transactions, events   │        │
│  │  • Indexes work submissions from oracles│        │
│  │  • Computes aggregate statistics        │        │
│  └────────────┬────────────────────────────┘        │
│               │                                       │
│               ▼                                       │
│  ┌─────────────────────────────────────────┐        │
│  │      PostgreSQL Database (Primary)      │        │
│  │  • Denormalized views for fast queries │        │
│  │  • User profiles and wallet links       │        │
│  │  • PoUW stats and leaderboards          │        │
│  │  • Governance participation history     │        │
│  │  • Disk-based storage (cost-effective) │        │
│  └────────────┬────────────────────────────┘        │
│               │                                       │
│               ▼                                       │
│  ┌─────────────────────────────────────────┐        │
│  │    moka In-Process Cache (Hot Data)     │        │
│  │  • 1-2GB RAM for frequently accessed   │        │
│  │  • Identities, leaderboards, proposals │        │
│  │  • Zero network latency                │        │
│  └────────────┬────────────────────────────┘        │
│               │                                       │
│               ▼                                       │
│  ┌─────────────────────────────────────────┐        │
│  │     REST/gRPC API Server (Public)       │        │
│  │  • /identity/* - User profiles          │        │
│  │  • /pouw/* - Work stats & leaderboards  │        │
│  │  • /governance/* - Proposal analytics   │        │
│  │  • /explorer/* - Block/tx queries       │        │
│  └─────────────────────────────────────────┘        │
└──────────────────────┬───────────────────────────────┘
                       │
                       │ HTTPS/gRPC
                       │
┌──────────────────────▼───────────────────────────────┐
│                Client Applications                    │
│  • Web/mobile/desktop wallets                       │
│  • Block explorers                                   │
│  • Governance dashboards                            │
│  • Third-party analytics tools                      │
└──────────────────────────────────────────────────────┘
```

---

## 🔧 Core Components

### 1. Chain Indexer Service

**Purpose**: Continuously sync blockchain data into PostgreSQL for fast queries

**Responsibilities**:
- Monitor new blocks and index transactions
- Track identity registry updates (profile changes, wallet links)
- Index governance proposals, votes, and outcomes
- Sync oracle work submissions (BOINC + NUW)
- Compute aggregate statistics (leaderboards, participation rates)
- Handle chain reorganizations gracefully

**Implementation**:
```rust
use moka::future::Cache;
use sqlx::PgPool;
use std::sync::Arc;

pub struct ChainIndexer {
    chain_client: ChertRpcClient,
    db_pool: PgPool,
    cache: Arc<ApiCache>,  // In-process cache for hot data
    last_synced_block: AtomicU64,
}

impl ChainIndexer {
    pub async fn sync_blocks(&self, from: u64, to: u64) -> Result<()>;
    pub async fn index_identities(&self, block: &Block) -> Result<()>;
    pub async fn index_work_submissions(&self, block: &Block) -> Result<()>;
    pub async fn index_governance(&self, block: &Block) -> Result<()>;
    pub async fn recompute_leaderboards(&self) -> Result<()>;
}
```

### 2. API Server

**Purpose**: Expose indexed data via REST and gRPC endpoints

**Key Endpoints**:

#### Identity & Profiles
- `GET /identity/{id}` - Get public profile by identity ID
- `GET /identity/{id}/wallets` - List linked wallet addresses
- `GET /identity/{id}/stats` - Get PoUW and governance stats
- `GET /identity/search?name={query}` - Search profiles by display name

#### PoUW Statistics
- `GET /pouw/stats/{identity_id}` - User's work contributions
- `GET /pouw/leaderboard?type={boinc|nuw}&timeframe={day|week|month|all}` - Top miners
- `GET /pouw/history/{identity_id}?limit=100` - Recent work submissions
- `GET /pouw/oracle/{oracle_id}/performance` - Oracle validation metrics

#### Governance
- `GET /governance/proposals?status={active|passed|rejected}` - List proposals
- `GET /governance/proposals/{id}` - Proposal details and vote breakdown
- `GET /governance/stats/{identity_id}` - Voting history and participation rate
- `GET /governance/power/{identity_id}` - Current voting power breakdown

#### Explorer
- `GET /explorer/blocks?limit=50&offset=0` - Recent blocks
- `GET /explorer/block/{height}` - Block details
- `GET /explorer/tx/{hash}` - Transaction details
- `GET /explorer/address/{addr}/balance` - Address balance
- `GET /explorer/address/{addr}/history` - Transaction history

**Rate Limiting**:
- Anonymous: 100 req/min
- Authenticated (via signature): 1000 req/min
- Caching: 10s for frequently accessed data

### 3. Database Schema

**Core Tables**:

```sql
-- User identity and profile data (denormalized from chain)
CREATE TABLE identities (
    identity_id BYTEA PRIMARY KEY,
    display_name TEXT,
    avatar_hash BYTEA,
    bio TEXT,
    stats_visibility TEXT NOT NULL DEFAULT 'private',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    INDEX idx_display_name (display_name)
);

-- Wallet addresses linked to identities
CREATE TABLE wallet_links (
    identity_id BYTEA NOT NULL,
    wallet_address TEXT NOT NULL,
    link_type TEXT NOT NULL, -- main, mining, staking, trading
    proof_signature BYTEA NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (identity_id, wallet_address),
    FOREIGN KEY (identity_id) REFERENCES identities(identity_id)
);

-- Aggregated PoUW statistics (computed by indexer)
CREATE TABLE pouw_stats (
    identity_id BYTEA PRIMARY KEY,
    total_nuw_completed BIGINT NOT NULL DEFAULT 0,
    total_boinc_credits DOUBLE PRECISION NOT NULL DEFAULT 0,
    work_breakdown JSONB NOT NULL DEFAULT '{}',
    rank_percentile DOUBLE PRECISION,
    active_since BIGINT,
    last_contribution BIGINT,
    updated_at BIGINT NOT NULL,
    FOREIGN KEY (identity_id) REFERENCES identities(identity_id)
);

-- Individual work submissions (indexed from chain)
CREATE TABLE work_submissions (
    submission_id BIGSERIAL PRIMARY KEY,
    miner_address TEXT NOT NULL,
    oracle_id TEXT NOT NULL,
    work_type TEXT NOT NULL, -- boinc, nuw_zkproof, nuw_dag, etc.
    difficulty DOUBLE PRECISION NOT NULL,
    reward BIGINT NOT NULL,
    validated_at BIGINT NOT NULL,
    block_height BIGINT NOT NULL,
    INDEX idx_miner (miner_address, validated_at DESC),
    INDEX idx_oracle (oracle_id, validated_at DESC),
    INDEX idx_work_type (work_type, validated_at DESC)
);

-- Governance proposals and votes
CREATE TABLE proposals (
    proposal_id BIGINT PRIMARY KEY,
    proposer_identity BYTEA NOT NULL,
    proposal_type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    voting_starts_at BIGINT NOT NULL,
    voting_ends_at BIGINT NOT NULL,
    status TEXT NOT NULL, -- active, passed, rejected, executed
    votes_for BIGINT DEFAULT 0,
    votes_against BIGINT DEFAULT 0,
    votes_abstain BIGINT DEFAULT 0,
    created_at BIGINT NOT NULL,
    INDEX idx_status (status, voting_ends_at DESC)
);

CREATE TABLE votes (
    vote_id BIGSERIAL PRIMARY KEY,
    proposal_id BIGINT NOT NULL,
    voter_identity BYTEA NOT NULL,
    vote_power BIGINT NOT NULL,
    vote_choice TEXT NOT NULL, -- for, against, abstain
    cast_at BIGINT NOT NULL,
    FOREIGN KEY (proposal_id) REFERENCES proposals(proposal_id),
    UNIQUE (proposal_id, voter_identity)
);

-- Governance participation metrics (computed)
CREATE TABLE governance_stats (
    identity_id BYTEA PRIMARY KEY,
    proposals_created BIGINT DEFAULT 0,
    votes_cast BIGINT DEFAULT 0,
    current_voting_power BIGINT DEFAULT 0,
    participation_rate DOUBLE PRECISION DEFAULT 0.0,
    delegation_received BIGINT DEFAULT 0,
    updated_at BIGINT NOT NULL,
    FOREIGN KEY (identity_id) REFERENCES identities(identity_id)
);

-- Social connections (optional feature)
CREATE TABLE social_connections (
    follower_id BYTEA NOT NULL,
    following_id BYTEA NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (follower_id, following_id),
    FOREIGN KEY (follower_id) REFERENCES identities(identity_id),
    FOREIGN KEY (following_id) REFERENCES identities(identity_id)
);
```

---

## 🛠️ Technology Stack

### Backend Framework
- **Axum** - Async HTTP server (REST endpoints)
- **Tonic** - gRPC server implementation
- **Tower** - Middleware (rate limiting, tracing, metrics)

### Database & Caching
- **PostgreSQL 15+** - Primary data store (disk-based, cost-effective)
- **SeaORM** - Async ORM with code-first migrations
  - Entity definitions in Rust code
  - Type-safe query builder
  - Auto-generated migrations from entities
  - Native async/await support
  - PostgreSQL advanced features (JSON, arrays, custom types)
- **SQLx** - Direct SQL queries when ORM limitations hit
- **PgBouncer** - Connection pooling for horizontal scaling
- **moka** - In-process cache (for single-instance deployments)
  - Zero network latency
  - Type-safe, compile-time checked
  - LRU/TTL eviction policies
  - 1-2GB RAM budget for hot data
- **Valkey** (optional, for multi-instance scaling)
  - Shared cache across multiple API servers
  - Session storage and rate limiting
  - Add when horizontally scaling beyond single instance

**Database Architecture**:
```rust
use sea_orm::entity::prelude::*;
use sea_orm::{Database, DatabaseConnection};

// Define entity (maps to PostgreSQL table)
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "identities")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub identity_id: Vec<u8>,
    pub display_name: Option<String>,
    pub avatar_hash: Option<Vec<u8>>,
    pub bio: Option<String>,
    pub stats_visibility: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::wallet_links::Entity")]
    WalletLinks,
    #[sea_orm(has_one = "super::pouw_stats::Entity")]
    PouwStats,
}

// Query with ORM
async fn get_identity(db: &DatabaseConnection, id: &[u8]) -> Result<Model> {
    identities::Entity::find_by_id(id.to_vec())
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Identity not found"))
}

// Use raw SQL for complex queries
async fn get_leaderboard(db: &DatabaseConnection) -> Result<Vec<LeaderboardEntry>> {
    LeaderboardEntry::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
        SELECT i.identity_id, i.display_name, p.total_nuw_completed
        FROM identities i
        JOIN pouw_stats p ON i.identity_id = p.identity_id
        WHERE i.stats_visibility = 'public'
        ORDER BY p.total_nuw_completed DESC
        LIMIT 100
        "#,
        [],
    ))
    .all(db)
    .await
}
```

**Caching Strategy**:
```rust
use moka::future::Cache;
use std::time::Duration;

pub struct ApiCache {
    // Hot user data (active identities, profiles)
    pub identities: Cache<IdentityId, Identity>,
    // Leaderboard top 100 (high traffic queries)
    pub leaderboards: Cache<String, Vec<LeaderboardEntry>>,
    // Recent governance proposals (frequently accessed)
    pub proposals: Cache<ProposalId, Proposal>,
}

impl ApiCache {
    pub fn new() -> Self {
        Self {
            identities: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(3600))  // 1 hour
                .time_to_idle(Duration::from_secs(600))   // 10 min idle
                .build(),
            leaderboards: Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(300))   // 5 min
                .build(),
            proposals: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(Duration::from_secs(1800))  // 30 min
                .build(),
        }
    }

    pub async fn get_identity(
        &self,
        db: &PgPool,
        id: &IdentityId,
    ) -> Result<Identity> {
        match self.identities.get(id).await {
            Some(identity) => Ok(identity),
            None => {
                let identity = db::fetch_identity(db, id).await?;
                self.identities.insert(*id, identity.clone()).await;
                Ok(identity)
            }
        }
    }
}
```

**When to Add Valkey**:
- Multiple API server instances (load balancing)
- Need cache consistency across servers
- Using cache for sessions, rate limiting, pub/sub
- Cache persistence across restarts is critical

### Blockchain Integration
- **silica-models** - Shared types with node
- **jsonrpsee** - RPC client for node communication
- **tokio** - Async runtime

### Observability
- **tracing** - Structured logging
- **prometheus** - Metrics export
- **OpenTelemetry** - Distributed tracing

### Deployment
- **Docker** - Containerization
- **Kubernetes** - Orchestration (optional, can run standalone)
- **Nginx** - Reverse proxy, TLS termination, HTTP caching

---

## 📋 Requirements & Dependencies

### Runtime Dependencies

**Required Services**:
- Chert full node (for RPC access to blockchain data)
- PostgreSQL 15+ (for data caching and aggregation)
- Oracle services (for PoUW work submission data)

**Optional Services** (for horizontal scaling):
- Valkey/Redis (shared cache across multiple API instances)
- Load balancer (HAProxy, Nginx, cloud LB)

**Minimum Hardware** (single instance):
- CPU: 4 cores (8+ recommended for indexing)
- RAM: 8GB (16GB recommended: 4GB app, 2GB cache, 2GB PostgreSQL, 8GB system)
- Storage: 100GB SSD (grows ~10GB/month with full indexing)
- Network: 1Gbps connection to Chert node

**Production Recommendations**:
- **Single instance** (testnet/early mainnet):
  - PostgreSQL on same host or managed service (RDS, Cloud SQL)
  - In-process moka cache (1-2GB)
  - Nginx reverse proxy with HTTP caching
- **Multi-instance** (high traffic):
  - 3+ API instances behind load balancer
  - Separate PostgreSQL cluster with replication
  - Valkey cluster for shared cache and rate limiting
  - CDN for static assets and cacheable responses

### Build Dependencies

```toml
[dependencies]
# Web framework
axum = "0.7"
tower = "0.4"
tower-http = "0.5"

# gRPC
tonic = "0.11"
prost = "0.12"

# Database & ORM
sea-orm = { version = "0.12", features = ["sqlx-postgres", "runtime-tokio-native-tls", "macros"] }
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-native-tls"] }

# In-process caching
moka = { version = "0.12", features = ["future"] }

# Optional: External cache (add when scaling horizontally)
# redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }

# Blockchain integration
silica-models = { path = "../silica-models" }
jsonrpsee = { version = "0.22", features = ["client"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Async runtime
tokio = { version = "1.36", features = ["full"] }

# Observability
tracing = "0.1"
tracing-subscriber = "0.3"
prometheus = "0.13"
opentelemetry = "0.22"

# Utilities
anyhow = "1.0"
thiserror = "1.0"
chrono = "0.4"
```

---

## 🚀 Development Setup

### 1. Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install PostgreSQL
sudo apt install postgresql-15 postgresql-client-15  # Ubuntu/Debian
# or
brew install postgresql@15  # macOS

# Install SeaORM CLI for migrations
cargo install sea-orm-cli
```

### 2. Database Setup

```bash
# Create database
createdb chert_api_dev

# Initialize SeaORM migration directory (first time only)
sea-orm-cli migrate init

# Generate migration from entity changes
sea-orm-cli migrate generate create_identities_table

# Run migrations
sea-orm-cli migrate up

# Optional: Generate entities from existing database
sea-orm-cli generate entity -o src/entities
```

**Migration Example** (`migration/src/m20241113_000001_create_identities.rs`):
```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Identities::Table)
                    .col(
                        ColumnDef::new(Identities::IdentityId)
                            .binary()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Identities::DisplayName).string())
                    .col(ColumnDef::new(Identities::AvatarHash).binary())
                    .col(ColumnDef::new(Identities::Bio).text())
                    .col(
                        ColumnDef::new(Identities::StatsVisibility)
                            .string()
                            .not_null()
                            .default("private"),
                    )
                    .col(ColumnDef::new(Identities::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(Identities::UpdatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_identities_display_name")
                    .table(Identities::Table)
                    .col(Identities::DisplayName)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Identities::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Identities {
    Table,
    IdentityId,
    DisplayName,
    AvatarHash,
    Bio,
    StatsVisibility,
    CreatedAt,
    UpdatedAt,
}
```

### 3. Configuration

Create `config/api.toml`:
```toml
[server]
host = "127.0.0.1"
port = 8080
grpc_port = 9090

[database]
url = "postgresql://localhost/chert_api_dev"
max_connections = 100
min_connections = 5

[chain]
rpc_url = "http://localhost:26657"  # Chert node RPC
sync_from_block = 0  # Or latest block to backfill from

[indexer]
enabled = true
batch_size = 100  # Blocks to index per batch
sync_interval_ms = 1000  # Check for new blocks every 1s

[rate_limiting]
anonymous_rpm = 100  # Requests per minute
authenticated_rpm = 1000

[cache]
# In-process moka cache settings
identities_max_capacity = 10_000
identities_ttl_seconds = 3600  # 1 hour
leaderboards_max_capacity = 100
leaderboards_ttl_seconds = 300  # 5 minutes
proposals_max_capacity = 1_000
proposals_ttl_seconds = 1800  # 30 minutes
```

### 4. Entity Definition

Create entity models in `src/entities/identities.rs`:
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "identities")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub identity_id: Vec<u8>,
    pub display_name: Option<String>,
    pub avatar_hash: Option<Vec<u8>>,
    pub bio: Option<String>,
    pub stats_visibility: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::wallet_links::Entity")]
    WalletLinks,
    #[sea_orm(has_one = "super::pouw_stats::Entity")]
    PouwStats,
}

impl Related<super::wallet_links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WalletLinks.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

### 5. Run Development Server

```bash
cd api

# Run migrations
sea-orm-cli migrate up

# Start API server
cargo run --bin chert-api-server

# In another terminal, run indexer
cargo run --bin chert-api-indexer
```

---

## 📊 API Versioning

**URL Structure**: `/v1/{resource}/{action}`

**Version Policy**:
- **v1**: Initial stable release (current)
- Backwards compatibility maintained for 6 months after new major version
- Breaking changes require new major version (v2, v3, etc.)
- Non-breaking additions can be added to existing versions

---

## 🔒 Security Considerations

**Authentication** (optional for most endpoints):
- Signature-based auth using user's identity keypair
- No passwords or API keys (self-sovereign identity)
- Rate limits apply to both authed and anonymous users

**Data Privacy**:
- API only serves **public on-chain data** (Tier 3 from wallet-ecosystem-design.md)
- Encrypted user data (Tier 2) never exposed via API
- Private keys never handled by API server

**DoS Protection**:
- Rate limiting per IP and per identity
- Query complexity limits (max rows, pagination required)
- Timeout on long-running queries (5s default)

---

## 📈 Monitoring & Operations

**Health Checks**:
- `GET /health` - Basic liveness check
- `GET /health/ready` - Ready to serve traffic (DB + chain connected)
- `GET /metrics` - Prometheus metrics

**Key Metrics**:
- Request rate and latency (p50, p95, p99)
- Indexer sync lag (blocks behind chain head)
- Database query performance
- Cache hit rate
- Error rate by endpoint

**Logging**:
- Structured JSON logs via `tracing`
- Correlation IDs for request tracing
- No sensitive data in logs (addresses, amounts OK; keys never)

---

## 🧪 Testing Strategy

**Unit Tests**:
- Database query logic
- Data transformation functions
- Rate limiting implementation

**Integration Tests**:
- Full API endpoint tests against test database
- Indexer tests against mock blockchain data
- End-to-end scenarios (create profile → query stats)

**Load Tests**:
- Target: 1000 req/s sustained with p95 < 100ms
- Chaos testing: DB failover, node disconnection

---

## 🛣️ Roadmap

### Phase 1: Foundation (Weeks 1-4)
- [x] Project structure and build system
- [x] Database schema and migrations
- [x] Basic chain indexer (blocks, transactions)
- [x] REST API framework with health checks

### Phase 2: Identity & Profiles (Weeks 5-8)
- [ ] Identity registry indexing
- [ ] Profile CRUD endpoints
- [ ] Wallet linkage verification
- [ ] Profile search functionality

### Phase 3: PoUW Stats (Weeks 9-12)
- [ ] Work submission indexing
- [ ] Aggregate stats computation
- [ ] Leaderboard generation
- [ ] Historical data queries

### Phase 4: Governance (Weeks 13-16)
- [ ] Proposal indexing
- [ ] Vote tracking and aggregation
- [ ] Voting power calculation
- [ ] Participation metrics

### Phase 5: Social Features (Weeks 17-20)
- [ ] Following/followers system
- [ ] Activity feed generation
- [ ] Notification hooks (webhooks)
- [ ] Privacy controls enforcement

### Phase 6: Production Hardening (Weeks 21-24)
- [ ] Horizontal scaling testing
- [ ] Performance optimization
- [ ] Security audit
- [ ] Production deployment

---

## 🤝 Contributing

**Prerequisites**:
- Read `docs/architecture/wallet-ecosystem-design.md` for context
- Familiarity with Rust async programming
- Understanding of REST/gRPC API design

**Development Workflow**:
1. Create feature branch from `main`
2. Write tests for new functionality
3. Ensure `cargo clippy` passes with zero warnings
4. Run `cargo fmt` before committing
5. Submit PR with clear description

**Code Standards**:
- Follow TigerBeetle-inspired memory safety (see SECURITY_GUIDELINES.md)
- All database queries must use compile-time checked SQLx
- API endpoints must have OpenAPI documentation
- Performance-critical paths require benchmarks

---

## 📚 Additional Resources

- [Wallet Ecosystem Design](../docs/architecture/wallet-ecosystem-design.md) - Full context on identity system
- [PoUW Architecture](../docs/architecture/pow-architecture.md) - Work submission validation
- [Governance Overview](../docs/governance/overview.md) - Voting power calculation
- [Privacy Architecture](../docs/technical/phase1-privacy-architecture.md) - Data classification tiers

---

**Status**: This is a **planning document**. Implementation has not yet started. API design is subject to change based on team review and security considerations.

