# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

The backend of the Linx app: an Alephium blockchain indexer plus REST API, built in Rust on PostgreSQL with Diesel.
The application lives in `crates/linx-indexer`. The `bento-*` crates are the indexing framework it was built on
(forked from bento-alephium, no longer synced upstream).

## Project Structure

Cargo workspace, every crate under `crates/`:

- `linx-indexer` - The application: processors (transfers, dex, lending; a contract-call processor exists but isn't
  registered), models, repositories, routers, services, binaries, migrations

`config.toml` (runtime tuning) is at the repository root.
- `bento-core` - Worker, block fetching client, pipeline, built-in block/event/transaction processors
- `bento-cli` - CLI (`run worker | backfill | server`) and TOML config loading
- `bento-server` - Axum server and Swagger UI
- `bento-types` - Shared types, chain-data models, chain-data migrations
- `bento-trait` - `ProcessorTrait`, `BlockProvider`

## Building and Running

```bash
cargo build
cargo build -p linx-indexer
```

The toolchain is pinned in `rust-toolchain.toml`; CI and the Docker images use the same version.

Run from the repository root. Binaries need the environment from `.env` (see `.env.example`) and read
`config.toml` from the current directory (`cli` also takes `-c` / `--config-path`):

```bash
cargo run -p linx-indexer --bin cli -- run worker
cargo run -p linx-indexer --bin cli -- run backfill --start 1716560632750 --stop 1716570000000
cargo run -p linx-indexer --bin cli -- run server
```

Other binaries: `snapshots`, `points`, `gap_detector` (`cargo run -p linx-indexer --bin <name>`).

### Environment

`.env` is loaded with `dotenvy`, and a line it can't parse silently stops loading every variable after it. Quote
values that contain spaces or quotes, e.g. `GAS_PAYER_ADDRESSES='["addr1", "addr2"]'`.

The database URL is built from `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB`, `POSTGRES_HOST` (default
`localhost`) and `POSTGRES_PORT` (default `5432`). `NETWORK` selects `devnet` / `testnet` / `mainnet`; `RPC_URL`
overrides the node URL.

### Migrations

There are two migration sets:

- `crates/bento-types/migrations` - chain-data tables (blocks, events, transactions, processor_status). Embedded in
  `bento-core` and applied when the worker starts.
- `crates/linx-indexer/migrations` - application tables. **Not** applied automatically; run with the Diesel CLI.

```bash
set -a; source .env; set +a
export DATABASE_URL="postgresql://$POSTGRES_USER:$POSTGRES_PASSWORD@${POSTGRES_HOST:-localhost}:${POSTGRES_PORT:-5432}/$POSTGRES_DB"
diesel setup --migration-dir crates/bento-types/migrations
diesel migration run --migration-dir crates/linx-indexer/migrations
```

The root `diesel.toml` regenerates a stray `src/schema.rs` at the repo root on every run; the real schema is
`crates/linx-indexer/src/schema.rs`.

Never edit a migration that has already been applied in production; add a new one. Existing databases won't rerun
it, so the edit only affects fresh databases and the two drift apart.

### Testing

Tests that need PostgreSQL are marked `#[ignore = "requires database"]`.

```bash
cargo test                          # unit tests only (CI)
cargo test -- --ignored             # database tests
cargo test -- --include-ignored     # both
```

Tests that read or set process environment variables must be `#[serial_test::serial]`; tests run in parallel and
share the environment.

## Architecture

### Processing Pipeline

1. **Worker** (`bento-core/src/workers/worker.rs`) - Orchestrates sync and backfill
2. **Client** - Fetches blocks and events from the Alephium node
3. **Pipeline** - Runs blocks through the registered processors
4. **Processors** - Turn blocks and events into models
5. **Storage** - Each processor persists its own output via Diesel

### Processor System

All processors implement `ProcessorTrait` (`bento-trait/src/processor.rs`):

```rust
#[async_trait]
pub trait ProcessorTrait: Send + Sync + Debug + 'static {
    fn name(&self) -> &'static str;
    fn connection_pool(&self) -> &Arc<DbPool>;
    async fn process_blocks(&self, blocks: Vec<BlockAndEvents>) -> Result<ProcessorOutput>;
    async fn store_output(&self, output: ProcessorOutput) -> Result<()>;
}
```

Application processors are registered in `get_processor_factories()` in `crates/linx-indexer/src/lib.rs` and
configured under `[processors.<name>]` in the TOML config (passed to the factory). Processors that output several
model types use `ProcessorOutput::Custom` and downcast in `store_output()`.

### Configuration

`config.toml` holds tuning only; addresses, URLs and other secrets come from `.env` via `AppConfig::from_env`
(`crates/linx-indexer/src/config.rs`).

- `[worker]` - Sync intervals, step sizes
- `[server]` - API port
- `[backfill]` - Worker count and intervals for historical sync
- `[points]` - Referral percentage, signup bonus, daily calculation time

The loader also accepts `[processors.{name}]` (passed to processor factories) and `[price_service]`, but the Linx
processors and services don't read them.

### Alephium-Specific Concepts

- **Groups** - Alephium shards into chain groups (4x4 = 16 chains)
- **Block ranges** - Defined by timestamps, not heights
- **Events** - Contract events are the main data source. `event_index` is the event's position in the contract's
  event declarations (its type), not its position in the transaction.

## Adding a Processor

1. Create it in `crates/linx-indexer/src/processors/`
2. Add models in `src/models/` and tables in `src/schema.rs`
3. Add a migration under `crates/linx-indexer/migrations/`
4. Register the factory in `get_processor_factories()` and add a config section
5. Test with a backfill over a small timestamp range

## Common Patterns

- Filter events by contract address before parsing.
- Batch inserts with `.values(&vec_of_models)`; make them idempotent with `on_conflict(...).do_nothing()`.
- `anyhow::Result` throughout; `tracing` for logs (initialized in `bento-cli/src/lib.rs`).

### Code Quality

When completing an implementation, run:

```bash
cargo clippy --fix --allow-dirty --allow-staged
```

CI runs `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features` and `cargo test --all-features`
with `RUSTFLAGS=-Dwarnings`.

## Deployment

- `docker-compose.yml` + `Taskfile.yml` - local stack (`task up`, `task down`)
- `docker-compose.prod.yml` + `Makefile` - production (`make deploy VERSION=x.y.z`, `make backup`, `make help`)
- `Dockerfile` (indexer/API) and `Dockerfile.cli` (CLI image with the Diesel CLI and migrations)
