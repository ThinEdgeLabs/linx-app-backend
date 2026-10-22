# Linx App Backend

Indexer and REST API behind the Linx app. It follows the Alephium blockchain, indexes transfers, DEX swaps
(Ayin, Elexium, PowFi) and lending activity into PostgreSQL, and serves account history, lending, stats and
points endpoints.

## Repository layout

| Path | What it is |
|---|---|
| `crates/linx-indexer` | The application: processors, models, repositories, HTTP routers, services, binaries and migrations |
| `config.toml` | Runtime tuning for the worker, backfill, server and points |
| `crates/bento-core` | Block fetching from the Alephium node, the worker that runs processors, built-in block/event/transaction processors |
| `crates/bento-cli` | Command-line entry point (`run worker`, `run backfill`, `run server`) and TOML config loading |
| `crates/bento-server` | Axum server setup and Swagger UI |
| `crates/bento-types` | Shared types, chain-data models and their migrations |
| `crates/bento-trait` | `ProcessorTrait` and the block provider traits |

The `bento-*` crates started as the [bento-alephium](https://github.com/ThinEdgeLabs/bento-alephium) indexing
framework; this repository no longer tracks it.

## Getting started

Requirements: the Rust toolchain from `rust-toolchain.toml` (installed automatically by `rustup`), Docker, and
the Diesel CLI (`cargo install diesel_cli --no-default-features --features postgres`).

```bash
cp .env.example .env                 # then fill in the Linx values
docker compose up -d db              # PostgreSQL only
```

Apply migrations. Chain-data tables come from `bento-types`, application tables from `linx-indexer`; run them in
this order:

```bash
set -a; source .env; set +a
export DATABASE_URL="postgresql://$POSTGRES_USER:$POSTGRES_PASSWORD@${POSTGRES_HOST:-localhost}:${POSTGRES_PORT:-5432}/$POSTGRES_DB"
diesel setup --migration-dir crates/bento-types/migrations
diesel migration run --migration-dir crates/linx-indexer/migrations
```

## Running

Run from the repository root. Tuning parameters are in `config.toml`, which every mode and binary reads from the
current directory (pass `-c` / `--config-path` to the `cli` binary to use another file). Addresses and other
secrets come from `.env`, not the config file.

```bash
# Follow the chain
cargo run -p linx-indexer --bin cli -- run worker

# Index a historical range (timestamps in milliseconds)
cargo run -p linx-indexer --bin cli -- run backfill --start <ms> --stop <ms>

# REST API, with Swagger UI at /swagger-ui
cargo run -p linx-indexer --bin cli -- run server
```

Other binaries in `crates/linx-indexer/src/bin`:

- `snapshots`: lending position and market snapshots, plus data retention cleanup
- `points`: points calculation
- `gap_detector`: finds and re-indexes missing blocks

## Testing

```bash
cargo test                          # unit tests, no database needed (what CI runs)
cargo test -- --ignored             # database tests, need PostgreSQL from .env
cargo clippy --all-targets --all-features
```

## Deployment

`docker-compose.yml` runs the full stack locally (`task up` / `task down` via `Taskfile.yml`).
Production uses `docker-compose.prod.yml` driven by the `Makefile`, e.g. `make deploy VERSION=1.12.0`; run
`make help` for the other targets.
