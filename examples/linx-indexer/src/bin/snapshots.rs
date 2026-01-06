use bento_cli::load_config;
use bento_core::new_db_pool;
use bento_types::network::Network;
use linx_indexer::services::{MarketStateSnapshotService, PositionSnapshotService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    let config_path = "config.toml";
    let config = load_config(config_path).expect("Failed to load config");

    let db_pool = new_db_pool(&config.worker.database_url, None).await?;

    let network: Network;
    if let Some(rpc_url) = &config.worker.rpc_url {
        network = Network::Custom(rpc_url.to_string(), config.worker.network.clone().into());
    } else {
        network = config.worker.network.clone().into();
    }

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
