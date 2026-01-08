use bento_cli::{get_database_url, get_network, load_config};
use bento_core::new_db_pool;
use chrono::NaiveDate;
use linx_indexer::services::PointsCalculatorService;
use linx_indexer::services::price::token_service::TokenService;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    let config_path = "config.toml";
    let config = load_config(config_path).expect("Failed to load config");

    let database_url = get_database_url().expect("DATABASE_URL must be set in environment");
    let db_pool = new_db_pool(&database_url, None).await?;

    let network = get_network().expect("NETWORK must be set in environment");

    let price_service = Arc::new(TokenService::new(network));

    let points_config = config
        .points
        .expect("Points configuration not found in config.toml. Add [points] section.");

    let calculator_service =
        PointsCalculatorService::new(db_pool.clone(), price_service, points_config.clone());

    match std::env::args().nth(1).as_deref() {
        Some("once") => {
            let date_str =
                std::env::args().nth(2).expect("Usage: points once <date> (format: YYYY-MM-DD)");
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .expect("Invalid date format. Use YYYY-MM-DD");

            tracing::info!("Calculating points for date: {}", date);
            calculator_service.calculate_points_for_date(date).await?;
            tracing::info!("Points calculation completed successfully");
        }
        Some("range") | Some("backfill") => {
            let start_str = std::env::args()
                .nth(2)
                .expect("Usage: points range <start-date> <end-date> (format: YYYY-MM-DD)");
            let end_str = std::env::args()
                .nth(3)
                .expect("Usage: points range <start-date> <end-date> (format: YYYY-MM-DD)");

            let start_date = NaiveDate::parse_from_str(&start_str, "%Y-%m-%d")
                .expect("Invalid start date format. Use YYYY-MM-DD");
            let end_date = NaiveDate::parse_from_str(&end_str, "%Y-%m-%d")
                .expect("Invalid end date format. Use YYYY-MM-DD");

            if start_date > end_date {
                anyhow::bail!("Start date must be before or equal to end date");
            }

            tracing::info!("Calculating points for range: {} to {}", start_date, end_date);
            calculator_service.calculate_points_for_range(start_date, end_date).await?;
            tracing::info!("Points calculation for range completed successfully");
        }
        Some("daemon") => {
            tracing::info!("Starting points calculator daemon...");
            tracing::info!("Will calculate points daily at: {}", points_config.calculation_time);
            calculator_service.run_scheduler().await?;
        }
        _ => {
            println!("Linx Points Calculator");
            println!();
            println!("Usage:");
            println!("  points once <date>              Calculate points for a specific date");
            println!("  points range <start> <end>      Calculate points for a date range");
            println!("  points backfill <start> <end>   Backfill historical points data");
            println!("  points daemon                   Run as scheduler (calculates daily)");
            println!();
            println!("Date format: YYYY-MM-DD (e.g., 2025-01-15)");
            println!();
            println!("Examples:");
            println!("  points once 2025-01-15");
            println!("  points range 2025-01-01 2025-01-15");
            println!("  points backfill 2024-12-01 2024-12-31");
            println!("  points daemon");
        }
    }

    Ok(())
}
