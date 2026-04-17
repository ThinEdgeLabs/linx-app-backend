use std::sync::Arc;

use bento_cli::{get_database_url, get_network};
use bento_core::new_db_pool;
use linx_indexer::config::AppConfig;
use linx_indexer::jobs::{PeriodicJob, run_job_forever};
use linx_indexer::services::price::token_service::TokenService;
use linx_indexer::services::{
    MarketApyService, MarketStateSnapshotService, PositionSnapshotService, StatsSnapshotService,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let app_config = AppConfig::from_env()?;
    let database_url = get_database_url().expect("DATABASE_URL must be set in environment");
    let db_pool = new_db_pool(&database_url, None).await?;
    let network = get_network().expect("NETWORK must be set in environment");

    let jobs = build_jobs(db_pool, network, &app_config);

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
        _ => print_usage(&jobs),
    }

    Ok(())
}

fn build_jobs(
    db_pool: Arc<bento_core::DbPool>,
    network: bento_types::network::Network,
    app_config: &AppConfig,
) -> Vec<Arc<dyn PeriodicJob>> {
    let token_service = TokenService::new(
        network.clone(),
        app_config.linx_api_url.clone(),
        app_config.dia_oracle_address.clone(),
        app_config.linx_group,
    );
    let client = bento_core::Client::new(network.clone());

    vec![
        Arc::new(PositionSnapshotService::new(
            db_pool.clone(),
            client,
            token_service,
            app_config.linx_address.clone(),
            app_config.linx_group,
        )),
        Arc::new(MarketStateSnapshotService::new(
            db_pool.clone(),
            network.clone(),
            app_config.linx_address.clone(),
            app_config.linx_group,
        )),
        Arc::new(MarketApyService::new(
            db_pool.clone(),
            network,
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
    println!("  run                 Run every registered job concurrently on its own interval");
    println!("  run-once <name>     Execute a single job's tick once and exit (useful for cron / backfill)");
    println!("  list                List registered jobs and their intervals");
    println!();
    println!("Registered jobs:");
    for j in jobs {
        println!("  {}", j.name());
    }
}
