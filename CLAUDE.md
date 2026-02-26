# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Bento Alephium is a custom blockchain data processor framework for indexing and processing Alephium blockchain data. The framework uses PostgreSQL with Diesel ORM for storage and provides a flexible architecture for handling different types of blockchain events.

## Project Structure

The repository is a Cargo workspace with the following structure:

- **crates/** - Core framework components:
  - `bento-trait` - Core traits (`ProcessorTrait`, `BlockProvider`)
  - `bento-types` - Shared types and data structures
  - `bento-core` - Main processing engine (Worker, Pipeline, Client, built-in processors)
  - `bento-cli` - CLI interface for running workers and servers
  - `bento-server` - HTTP server with REST API and Swagger UI

- **examples/** - Reference implementations:
  - `linx-indexer` - Production indexer with custom processors (transfers, dex, lending, contract calls)
  - `lending-indexer` - Simpler lending marketplace example
  - `event-indexer` - Basic event processor example

## Building and Running

### Build Commands
```bash
# Build all workspace crates
cargo build

# Build specific example
cargo build -p linx-indexer

# Build with release optimizations
cargo build --release
```

### Running the Indexer

The main binary is in `examples/linx-indexer/`. It requires:
1. Environment variables (POSTGRES_*, NETWORK) - set in `.env` file
2. Config file (TOML format) - for tuning parameters

```bash
# Ensure .env file exists with required variables
cat .env  # Should contain POSTGRES_USER, POSTGRES_PASSWORD, POSTGRES_DB, and NETWORK

# Run in worker mode (real-time sync)
cargo run -p linx-indexer -- run worker --config examples/linx-indexer/config.toml

# Run in backfill mode (historical data)
cargo run -p linx-indexer -- run backfill --config examples/linx-indexer/config.toml --start 1716560632750 --stop 1716570000000

# Run in server mode (REST API)
cargo run -p linx-indexer -- run server --config examples/linx-indexer/config.toml
```

### Database Setup

The application constructs the database connection string from individual `POSTGRES_*` environment variables. This provides a single source of truth for both Docker Compose and the Rust application.

#### Required Environment Variables

Set these in your `.env` file:
```bash
POSTGRES_USER=postgres
POSTGRES_PASSWORD=postgres
POSTGRES_DB=bento_alephium
POSTGRES_HOST=localhost  # defaults to localhost if not set
POSTGRES_PORT=5432        # defaults to 5432 if not set
```

The application automatically constructs the connection string as:
```
postgresql://POSTGRES_USER:POSTGRES_PASSWORD@POSTGRES_HOST:POSTGRES_PORT/POSTGRES_DB
```

#### Docker/Container Environments

For production deployments, set these environment variables:
```yaml
# docker-compose.yml
environment:
  - POSTGRES_USER=myuser
  - POSTGRES_PASSWORD=mypassword
  - POSTGRES_DB=mydb
  - POSTGRES_HOST=db
  - POSTGRES_PORT=5432
```

#### Migrations

Migrations run automatically when the worker starts (see `worker.rs:293`).

For manual migration management:
```bash
# Install diesel CLI if needed
cargo install diesel_cli --no-default-features --features postgres

# Run migrations manually (from examples/linx-indexer/)
diesel migration run --config-file diesel.toml
```

### Testing

Tests that require a running PostgreSQL database are marked with `#[ignore = "requires database"]`.
By default, `cargo test` skips these and runs only unit tests (which use mocks).

```bash
# Run unit tests only (default, no database needed — used in CI)
cargo test

# Run database integration tests only (requires PostgreSQL)
cargo test -- --ignored

# Run all tests (unit + database)
cargo test -- --include-ignored

# Run tests for a specific crate
cargo test -p bento-core
```

## Architecture

### Processing Pipeline

The framework follows this flow:
1. **Worker** (`bento-core/src/workers/worker.rs`) - Orchestrates the entire process
2. **Client** - Fetches blocks and events from Alephium node
3. **Pipeline** - Processes blocks through registered processors
4. **Processors** - Transform blockchain data into domain models
5. **Storage** - Persist models to PostgreSQL via Diesel

### Processor System

All processors implement `ProcessorTrait` (defined in `bento-trait/src/processor.rs`):
```rust
#[async_trait]
pub trait ProcessorTrait: Send + Sync + Debug + 'static {
    fn name(&self) -> &'static str;
    fn connection_pool(&self) -> &Arc<DbPool>;
    async fn process_blocks(&self, blocks: Vec<BlockAndEvents>) -> Result<ProcessorOutput>;
    async fn store_output(&self, output: ProcessorOutput) -> Result<()>;
}
```

**Built-in Processors** (in `bento-core/src/processors/`):
- `BlockProcessor` - Indexes raw blocks
- `EventProcessor` - Indexes raw events
- `TxProcessor` - Indexes raw transactions

**Custom Processors** are registered via factory functions. See `examples/linx-indexer/src/bin/main.rs` for the registration pattern:
```rust
let mut processor_factories = HashMap::new();
processor_factories.insert("transfers".to_string(), transfer_processor::processor_factory());
processor_factories.insert("lending".to_string(), lending_processor::processor_factory());
```

### Configuration System

The CLI uses TOML configuration files (see `examples/linx-indexer/config.toml`):
- `[worker]` - Database URL, network, sync intervals, step sizes
- `[server]` - API server port
- `[backfill]` - Workers count, intervals for historical sync
- `[processors.{name}]` - Custom processor-specific config

Processor config is passed to factory functions as `serde_json::Value` args.

### Network Configuration

The framework supports multiple Alephium networks via the `NETWORK` environment variable:
- `testnet` - Test network
- `mainnet` - Production network
- `devnet` - Development network

Set in `.env` file:
```bash
NETWORK=testnet
```

**Custom RPC endpoint** (optional):
```bash
RPC_URL=https://your-custom-node.example.com
```

## Creating Custom Processors

Follow these steps (detailed guide in `crates/bento-core/src/processors/README.md`):

1. **Define data models** using Diesel derives:
   ```rust
   #[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset)]
   #[diesel(table_name = schema::your_table)]
   pub struct YourModel { /* fields */ }
   ```

2. **Implement ProcessorTrait**:
   - `process_blocks()` - Convert blockchain data to models
   - `store_output()` - Save models to database (handle custom output types)

3. **Create factory function**:
   ```rust
   pub fn processor_factory() -> ProcessorFactory {
       Box::new(|pool, args| {
           Box::new(YourProcessor::new(pool, args))
       })
   }
   ```

4. **Register in main.rs** and add processor config to TOML

5. **Create migrations** for new tables (place in `migrations/` directory)

### Custom Output Types

For complex processors that output multiple model types, use `ProcessorOutput::Custom`:
```rust
impl CustomProcessorOutput for YourOutput {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> { Box::new(self.clone()) }
}
```

Then downcast in `store_output()` to access your specific output type.

## Key Implementation Details

### Worker Modes

- **Sync mode** (`run_sync`) - Continuous polling for new blocks with configurable backstep
- **Backfill mode** (`run_backfill`) - Historical data processing with start/stop timestamps
- **Server mode** - Exposes REST API (routes defined per processor)

Workers automatically run migrations via `run_migrations()` using the `libpq` feature.

### Parallel Fetching

The framework supports parallel block fetching (`fetch_parallel` in `workers/fetch.rs`) with configurable worker counts. Used in backfill mode for performance.

### Alephium-Specific Concepts

- **Groups** - Alephium uses chain groups for sharding (default: 4x4 = 16 chains)
- **Block ranges** - Defined by timestamps, not heights
- **Events** - Smart contract events are the primary data source for custom processors

## Binary Targets

The `linx-indexer` example has multiple binaries:
- `main.rs` - Main indexer with full processor registration
- `deposit_snapshots.rs` - Specialized snapshot generator

Run with: `cargo run -p linx-indexer --bin <binary_name>`

## Common Patterns

### Event Filtering
Filter events by contract address before processing to avoid processing irrelevant data.

### Batch Inserts
Use Diesel's `.values(&vec_of_models)` for efficient batch inserts.

### Error Handling
Use `anyhow::Result` throughout. The pipeline continues processing other ranges if one fails.

### Logging
Use `tracing::info!`, `tracing::warn!`, `tracing::error!` - initialized in `bento-cli/src/lib.rs:175`.

## Development Workflow

1. Create new processor in `examples/linx-indexer/src/processors/`
2. Define models and schema in `src/models/` and `src/schema.rs`
3. Create migration: `diesel migration generate your_migration_name`
4. Implement processor and factory function
5. Register in `src/bin/main.rs`
6. Add config section to `config.toml`
7. Test with backfill on small timestamp range
8. Deploy worker for continuous sync

### Code Quality

When completing an implementation, always run:
```bash
cargo clippy --fix --allow-dirty --allow-staged
```

This automatically applies Clippy's lint suggestions to improve code quality, readability, and adherence to Rust best practices.

## Docker Support

The repository includes:
- `docker-compose.yml` - PostgreSQL setup
- `Dockerfile.dev` - Development container (if needed)
- `entrypoint.sh` - Container initialization script
