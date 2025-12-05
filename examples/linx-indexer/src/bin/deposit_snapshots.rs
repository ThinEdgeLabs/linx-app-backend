use bento_cli::load_config;
use bento_core::new_db_pool;
use bento_types::network::Network;
use linx_indexer::services::DepositSnapshotService;

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

    let snapshot_service = DepositSnapshotService::new(db_pool, network);

    match std::env::args().nth(1).as_deref() {
        Some("once") => {
            snapshot_service.generate_snapshots().await?;
        }
        Some("daemon") => {
            snapshot_service.run_scheduler().await?;
        }
        _ => {
            println!("Usage: deposit_snapshots [once|daemon]");
        }
    }

    Ok(())
}
