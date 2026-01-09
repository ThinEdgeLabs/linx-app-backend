use bento_cli::{get_database_url, get_network};
use bento_core::new_db_pool;
use linx_indexer::config::AppConfig;
use linx_indexer::services::price::token_service::TokenService;
use linx_indexer::services::{MarketStateSnapshotService, PositionSnapshotService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    // Load application configuration from environment variables
    let app_config = AppConfig::from_env()?;

    let database_url = get_database_url().expect("DATABASE_URL must be set in environment");
    let db_pool = new_db_pool(&database_url, None).await?;

    let network = get_network().expect("NETWORK must be set in environment");

    match (std::env::args().nth(1).as_deref(), std::env::args().nth(2).as_deref()) {
        (Some("positions"), Some("once")) => {
            let token_service = TokenService::new(
                network.clone(),
                app_config.linx_api_url.clone(),
                app_config.dia_oracle_address.clone(),
                app_config.linx_group,
            );
            let client = bento_core::Client::new(network);
            let snapshot_service = PositionSnapshotService::new(db_pool, client, token_service);
            snapshot_service
                .generate_snapshots(&app_config.linx_address, app_config.linx_group)
                .await?;
        }
        (Some("positions"), Some("daemon")) => {
            let token_service = TokenService::new(
                network.clone(),
                app_config.linx_api_url.clone(),
                app_config.dia_oracle_address.clone(),
                app_config.linx_group,
            );
            let client = bento_core::Client::new(network);
            let snapshot_service = PositionSnapshotService::new(db_pool, client, token_service);
            snapshot_service.run_scheduler(&app_config.linx_address, app_config.linx_group).await?;
        }
        (Some("market-state"), Some("once")) => {
            let snapshot_service = MarketStateSnapshotService::new(
                db_pool,
                network,
                app_config.linx_address.clone(),
                app_config.linx_group,
            );
            snapshot_service.generate_snapshots().await?;
        }
        (Some("market-state"), Some("daemon")) => {
            let snapshot_service = MarketStateSnapshotService::new(
                db_pool,
                network,
                app_config.linx_address.clone(),
                app_config.linx_group,
            );
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
