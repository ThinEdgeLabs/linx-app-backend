use bento_cli::{get_database_url, get_network};
use bento_core::new_db_pool;
use linx_indexer::services::{MarketStateSnapshotService, PositionSnapshotService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    let database_url = get_database_url().expect("DATABASE_URL must be set in environment");
    let db_pool = new_db_pool(&database_url, None).await?;

    let network = get_network().expect("NETWORK must be set in environment");

    match (std::env::args().nth(1).as_deref(), std::env::args().nth(2).as_deref()) {
        (Some("positions"), Some("once")) => {
            let snapshot_service = PositionSnapshotService::new(db_pool, network);
            snapshot_service.generate_snapshots().await?;
        }
        (Some("positions"), Some("daemon")) => {
            let snapshot_service = PositionSnapshotService::new(db_pool, network);
            snapshot_service.run_scheduler().await?;
        }
        (Some("market-state"), Some("once")) => {
            let snapshot_service = MarketStateSnapshotService::new(db_pool, network);
            snapshot_service.generate_snapshots().await?;
        }
        (Some("market-state"), Some("daemon")) => {
            let snapshot_service = MarketStateSnapshotService::new(db_pool, network);
            snapshot_service.run_scheduler().await?;
        }
        _ => {
            println!("Usage: snapshots [positions|market-state] [once|daemon]");
            println!("  positions once     - Generate position snapshots once");
            println!("  positions daemon   - Run position snapshot scheduler");
            println!("  market-state once  - Generate market state snapshots once");
            println!("  market-state daemon - Run market state snapshot scheduler");
        }
    }

    Ok(())
}
