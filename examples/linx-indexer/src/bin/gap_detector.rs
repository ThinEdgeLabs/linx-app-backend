use bento_cli::load_config;
use bento_core::new_db_pool;
use bento_core::workers::worker::{BackfillOptions, SyncOptions, Worker};
use bento_types::network::Network;
use linx_indexer::{get_processor_factories, services::GapDetectionService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    //let config_path = std::env::args().nth(2).unwrap_or_else(|| "config.toml".to_string());
    let config_path = "config.toml";
    let config = load_config(&config_path).expect("Failed to load config");

    let db_pool = new_db_pool(&config.worker.database_url, None).await?;

    let gap_service = GapDetectionService::new(db_pool.clone());

    // Parse optional --min-height parameter
    let min_height: Option<i64> = std::env::args()
        .find(|arg| arg.starts_with("--min-height="))
        .and_then(|arg| arg.strip_prefix("--min-height=").and_then(|val| val.parse().ok()));

    // Parse optional --delay parameter (in milliseconds)
    let delay_ms: Option<u64> = std::env::args()
        .find(|arg| arg.starts_with("--delay="))
        .and_then(|arg| arg.strip_prefix("--delay=").and_then(|val| val.parse().ok()));

    if let Some(h) = min_height {
        println!("Using minimum height filter: {}", h);
    }

    if let Some(d) = delay_ms {
        println!("Using delay between heights: {}ms", d);
    }

    match std::env::args().nth(1).as_deref() {
        Some("detect") => {
            tracing::info!("Running gap detection");
            let report = gap_service.generate_report(min_height).await?;

            println!("\n=== Gap Detection Report ===");
            println!("Total missing blocks: {}", report.total_missing_blocks);
            println!("Chains with gaps: {}\n", report.block_gaps.len());

            if report.total_missing_blocks == 0 {
                println!("No gaps found! Database is complete.");
                return Ok(());
            }

            for gap in &report.block_gaps {
                println!(
                    "Chain ({}, {}): {} missing heights",
                    gap.chain_from, gap.chain_to, gap.total_missing
                );
                if gap.total_missing <= 20 {
                    println!("  Heights: {:?}", gap.missing_heights);
                } else {
                    println!("  First 10: {:?}", &gap.missing_heights[..10]);
                    println!("  Last 10: {:?}", &gap.missing_heights[gap.total_missing - 10..]);
                }
                println!();
            }

            println!("Run 'gap_detector backfill' to fix these gaps.");
        }
        Some("backfill") => {
            tracing::info!("Running gap backfill");

            // Create network
            let network: Network = if let Some(rpc_url) = &config.worker.rpc_url {
                Network::Custom(rpc_url.to_string(), config.worker.network.clone().into())
            } else {
                config.worker.network.clone().into()
            };

            // Get processor factories from shared function
            let processor_factories = get_processor_factories();

            // Build processor configs from the config
            let processor_configs: Vec<_> = config
                .processors
                .as_ref()
                .map(|p| {
                    p.processors
                        .iter()
                        .filter_map(|(name, processor_config)| {
                            processor_factories.get(name).map(|factory| {
                                bento_core::config::ProcessorConfig::custom(
                                    name.clone(),
                                    *factory,
                                    Some(serde_json::to_value(processor_config).unwrap()),
                                )
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            tracing::info!("Registered {} processors for backfill", processor_configs.len());

            // Create worker
            let worker = Worker::new(
                processor_configs,
                config.worker.database_url.clone(),
                network,
                None,
                Some(SyncOptions::default()),
                Some(BackfillOptions::default()),
                1,
            )
            .await?;

            // Run backfill
            gap_service.backfill_gaps(&worker, min_height, delay_ms).await?;

            println!("\n=== Backfill Complete ===");
            println!("All gaps have been backfilled successfully");
        }
        _ => {
            println!("Usage: gap_detector [detect|backfill] [--min-height=N] [--delay=MS]");
            println!();
            println!("Commands:");
            println!("  detect      - Detect and report missing block heights");
            println!("  backfill    - Backfill all detected gaps through ALL processors");
            println!();
            println!("Arguments:");
            println!("  --min-height=N  - Only detect/backfill gaps at or above this height");
            println!("                    (useful for dapps deployed after genesis)");
            println!("  --delay=MS      - Delay in milliseconds between each height during backfill");
            println!("                    (default: 100ms, helps avoid 503 errors from node)");
            println!();
            println!("Examples:");
            println!("  gap_detector detect");
            println!("  gap_detector backfill");
            println!("  gap_detector detect --min-height=1000000");
            println!("  gap_detector backfill --min-height=1000000");
            println!("  gap_detector backfill --delay=200");
            println!("  gap_detector backfill --min-height=1000000 --delay=200");
        }
    }

    Ok(())
}
