use std::sync::Arc;

use bento_cli::{get_database_url, get_network};
use bento_core::new_db_pool;
use chrono::DateTime;
use linx_indexer::config::AppConfig;
use linx_indexer::jobs::{PeriodicJob, run_job_forever};
use linx_indexer::repository::LendingRepository;
use linx_indexer::services::price::token_service::TokenService;
use linx_indexer::services::{MarketStateSnapshotService, PositionSnapshotService, StatsSnapshotService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let app_config = AppConfig::from_env()?;
    let database_url = get_database_url().expect("DATABASE_URL must be set in environment");
    let db_pool = new_db_pool(&database_url, None).await?;
    let network = get_network().expect("NETWORK must be set in environment");

    let token_service = Arc::new(TokenService::new(
        network.clone(),
        app_config.linx_api_url.clone(),
        app_config.dia_oracle_address.clone(),
        app_config.linx_group,
    ));

    let jobs = build_jobs(db_pool.clone(), network, &app_config, token_service.clone());

    match (std::env::args().nth(1).as_deref(), std::env::args().nth(2).as_deref()) {
        (Some("run"), None) => {
            let mut set = tokio::task::JoinSet::new();
            for job in jobs {
                set.spawn(run_job_forever(job));
            }
            while let Some(res) = set.join_next().await {
                res??;
            }
        }
        (Some("run-once"), Some(name)) => {
            let job =
                jobs.into_iter().find(|j| j.name() == name).ok_or_else(|| anyhow::anyhow!("unknown job: {name}"))?;
            job.tick().await?;
        }
        (Some("list"), None) => {
            for j in &jobs {
                println!("{:<28} every {:?}", j.name(), j.interval());
            }
        }
        (Some("backfill"), Some(metric)) => {
            let repo = LendingRepository::new(db_pool);
            match metric {
                "cumulative-volumes" => backfill_cumulative_volumes(&repo, &token_service).await?,
                other => anyhow::bail!("unknown backfill metric: {other}"),
            }
        }
        _ => print_usage(&jobs),
    }

    Ok(())
}

/// Recompute `cumulative_supply_volume_usd` / `cumulative_borrow_volume_usd` for every existing
/// snapshot row using SUM-from-inception, priced at the current loan-token price. Idempotent.
async fn backfill_cumulative_volumes(
    repo: &LendingRepository,
    token_service: &Arc<TokenService>,
) -> anyhow::Result<()> {
    let epoch = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
    let markets = repo.get_all_markets().await?;
    tracing::info!("Backfilling cumulative volumes for {} markets", markets.len());

    let mut total_updated = 0usize;
    for market in markets {
        let loan_info = token_service.get_token_info(&market.loan_token).await?;
        let loan_price = token_service.get_token_price(&market.loan_token).await?;
        let rows = repo.list_snapshot_ids_for_market(&market.id).await?;
        tracing::info!("market {}: {} snapshots to update", market.id, rows.len());

        for (snapshot_id, snapshot_ts) in rows {
            let supply_amt = repo.sum_event_amounts(&market.id, "Supply", epoch, snapshot_ts).await?;
            let borrow_amt = repo.sum_event_amounts(&market.id, "Borrow", epoch, snapshot_ts).await?;
            let supply_usd = (loan_info.convert_to_decimal(&supply_amt) * &loan_price).with_scale(2);
            let borrow_usd = (loan_info.convert_to_decimal(&borrow_amt) * &loan_price).with_scale(2);
            repo.update_cumulative_volumes(snapshot_id, &supply_usd, &borrow_usd).await?;
            total_updated += 1;
        }
    }

    tracing::info!("Backfill complete. Updated {} rows.", total_updated);
    Ok(())
}

fn build_jobs(
    db_pool: Arc<bento_core::DbPool>,
    network: bento_types::network::Network,
    app_config: &AppConfig,
    token_service: Arc<TokenService>,
) -> Vec<Arc<dyn PeriodicJob>> {
    let client = bento_core::Client::new(network.clone());

    vec![
        Arc::new(PositionSnapshotService::new(
            db_pool.clone(),
            client,
            token_service.clone(),
            app_config.linx_address.clone(),
            app_config.linx_group,
        )),
        Arc::new(MarketStateSnapshotService::new(
            db_pool.clone(),
            network,
            token_service,
            app_config.linx_address.clone(),
            app_config.linx_group,
        )),
        Arc::new(StatsSnapshotService::new(db_pool)),
    ]
}

fn print_usage(jobs: &[Arc<dyn PeriodicJob>]) {
    println!("Usage: snapshots <command>");
    println!();
    println!("Commands:");
    println!("  run                  Run every registered job concurrently on its own interval");
    println!("  run-once <name>      Execute a single job's tick once and exit (useful for cron)");
    println!("  list                 List registered jobs and their intervals");
    println!("  backfill <metric>    Recompute a metric across existing snapshot rows");
    println!();
    println!("Registered jobs:");
    for j in jobs {
        println!("  {}", j.name());
    }
    println!();
    println!("Backfill metrics:");
    println!("  cumulative-volumes   Recompute cumulative_supply/borrow_volume_usd from inception");
}
