pub mod constants;
pub mod types;
use crate::types::*;
use bento_types::network::Network;
use clap::Parser;

use anyhow::{Context, Result};
use bento_core::{
    config::ProcessorConfig,
    new_db_pool,
    worker::{BackfillOptions, SyncOptions},
    workers::worker::Worker,
    Client, ProcessorFactory,
};
use bento_server::{start, AppState, Config as ServerConfig};
use std::{collections::HashMap, fs, path::Path, sync::Arc};
use utoipa_axum::router::OpenApiRouter;

/// Get database URL constructed from POSTGRES_* environment variables
pub fn get_database_url() -> Result<String> {
    let user = std::env::var("POSTGRES_USER").context(
        "POSTGRES_USER environment variable not set. \
        Please set POSTGRES_USER in .env file"
    )?;

    let password = std::env::var("POSTGRES_PASSWORD").context(
        "POSTGRES_PASSWORD environment variable not set. \
        Please set POSTGRES_PASSWORD in .env file"
    )?;

    let db = std::env::var("POSTGRES_DB").context(
        "POSTGRES_DB environment variable not set. \
        Please set POSTGRES_DB in .env file"
    )?;

    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());

    Ok(format!(
        "postgresql://{}:{}@{}:{}/{}",
        user, password, host, port, db
    ))
}

/// Get network from NETWORK environment variable (testnet, mainnet, or devnet)
pub fn get_network() -> Result<Network> {
    let network_str = std::env::var("NETWORK").context(
        "NETWORK environment variable not set. \
        Please set NETWORK in .env file or environment. \
        Example: export NETWORK='testnet'"
    )?;

    // Check for optional RPC_URL
    let rpc_url = std::env::var("RPC_URL").ok();

    if let Some(url) = rpc_url {
        Ok(Network::Custom(url, network_str.into()))
    } else {
        Ok(network_str.into())
    }
}

async fn new_worker_from_config(
    _config: &Config,
    processor_factories: &HashMap<String, ProcessorFactory>,
    include_default_processors: bool,
    workers: usize,
    sync_options: Option<SyncOptions>,
    backfill_options: Option<BackfillOptions>,
    app_config: Option<Arc<dyn bento_types::config::AppConfigTrait>>,
) -> Result<Worker> {
    let mut processors = Vec::new();

    if include_default_processors {
        processors.extend(vec![
            ProcessorConfig::BlockProcessor,
            ProcessorConfig::EventProcessor,
            ProcessorConfig::TxProcessor,
        ]);
    }

    for (processor_name, processor_factory) in processor_factories.iter() {
        let processor_config = ProcessorConfig::Custom {
            name: processor_name.clone(),
            factory: *processor_factory,
            config: app_config.clone(),
        };
        processors.push(processor_config);
    }

    let network = get_network()?;

    let worker = Worker::new(
        processors,
        get_database_url()?,
        network,
        None,
        sync_options,
        backfill_options,
        workers,
    )
    .await?;
    Ok(worker)
}

pub async fn new_realtime_worker_from_config(
    config: &Config,
    processor_factories: &HashMap<String, ProcessorFactory>,
    include_default_processors: bool,
    app_config: Option<Arc<dyn bento_types::config::AppConfigTrait>>,
) -> Result<Worker> {
    let workers: usize = 2;
    let step = config.worker.step;
    let backstep = config.worker.backstep;
    let request_interval = config.worker.request_interval;

    new_worker_from_config(
        config,
        processor_factories,
        include_default_processors,
        workers,
        Some(SyncOptions { step, backstep, request_interval }),
        None,
        app_config,
    )
    .await
}

pub async fn new_backfill_worker_from_config(
    start_ts: Option<u64>,
    stop_ts: Option<u64>,
    config: &Config,
    processor_factories: &HashMap<String, ProcessorFactory>,
    include_default_processors: bool,
    app_config: Option<Arc<dyn bento_types::config::AppConfigTrait>>,
) -> Result<Worker> {
    let workers = config.backfill.workers;
    let step = config.backfill.step;
    let backstep = config.backfill.backstep;
    let request_interval = config.backfill.request_interval;

    new_worker_from_config(
        config,
        processor_factories,
        include_default_processors,
        workers,
        None,
        Some(BackfillOptions { start_ts, stop_ts, step, backstep, request_interval }),
        app_config,
    )
    .await
}

pub async fn new_server_config_from_config(_config: &Config) -> Result<ServerConfig> {
    let database_url = get_database_url()?;
    let db_pool = new_db_pool(&database_url, None).await?;

    let network = get_network()?;
    let client = Arc::new(Client::new(network));

    let api_host = std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let api_port = std::env::var("API_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .context("Invalid API_PORT value")?;

    let server_config = ServerConfig {
        db_client: db_pool,
        node_client: client,
        api_host,
        api_port,
    };
    Ok(server_config)
}

/// Main function to run the command line interface
///
/// This function serves as the entry point for the Bento application's CLI.
/// It handles parsing command-line arguments and executing the appropriate
/// functionality based on the provided commands and options.
///
/// # Arguments
///
/// * `processor_factories` - A HashMap containing custom processor factories,
///   where the key is the processor name and the value is the processor factory function.
/// * `include_default_processors` - A boolean flag indicating whether to include
///   the default processors (block, event, and tx) in addition to any custom processors.
///
/// # Returns
///
/// * `Result<()>` - Returns Ok(()) if successful, or an error if any operation fails.
///
/// # Commands
///
/// The function supports various subcommands through the CLI:
/// * `Run` - Executes the application in different modes:
///   * `Server` - Runs in server mode.
///   * `Worker` - Runs in worker mode with specified processors.
///   * `Backfill` - Performs data backfilling for specified processors.
///   * `BackfillStatus` - Displays backfill status for a specific processor.
///
/// # Examples
///
/// ```
// / let processor_factories = HashMap::new();
// / run_command(processor_factories, true).await?;
/// ```
pub async fn run_command(
    processor_factories: HashMap<String, ProcessorFactory>,
    router: Option<OpenApiRouter<AppState>>,
    include_default_processors: bool,
    app_config: Option<Arc<dyn bento_types::config::AppConfigTrait>>,
) -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Run(run) => match run.mode {
            RunMode::Server(args) => {
                let config = args.clone().into();
                let server_config = new_server_config_from_config(&config).await?;

                println!("Server is ready and running on http://{}", server_config.api_endpoint());
                println!(
                    "Swagger UI is available at http://{}/swagger-ui",
                    server_config.api_endpoint()
                );

                start(server_config, router).await?;
            }
            RunMode::Worker(args) => {
                let config = args.clone().into();

                println!("⚙️  Running real-time indexer with config: {}", args.config_path);

                let worker = new_realtime_worker_from_config(
                    &config,
                    &processor_factories,
                    include_default_processors,
                    app_config.clone(),
                )
                .await?;

                println!("🚀 Starting real-time indexer");

                worker.run().await?;
            }
            RunMode::Backfill(args) => {
                let config = args.clone().into();

                let worker = new_backfill_worker_from_config(
                    args.start,
                    args.stop,
                    &config,
                    &processor_factories,
                    include_default_processors,
                    app_config.clone(),
                )
                .await?;

                println!("Starting backfill worker...");
                worker.run().await?;
            }
        },
    }
    Ok(())
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let content = fs::read_to_string(path).context("Failed to read config file")?;
    let config: Config = toml::from_str(&content).context("Failed to parse config file")?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_test_config_file(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let config_path = dir.join("test_config.toml");
        let mut file = File::create(&config_path).expect("Failed to create test config file");
        file.write_all(content.as_bytes()).expect("Failed to write to test config file");
        config_path
    }

    #[test]
    fn test_load_config() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let config_content = r#"
            [worker]
            request_interval = 500
            step = 60000
            backstep = 300000

            [server]

            [backfill]
            request_interval = 1000
            workers = 2
            step = 1800000
            backstep = 600000

            [processors.custom_processor]
            field1 = "value1"
            field2 = 42
        "#;

        let config_path = create_test_config_file(temp_dir.path(), config_content);

        // Create CLI args with the path to our test config
        let args = CliArgs {
            config_path: config_path.to_string_lossy().to_string(),
            network: Some("testnet".to_string()),
        };

        let config: Config = args.clone().into();

        // Verify the config was loaded correctly
        assert_eq!(config.worker.request_interval, 500);

        assert_eq!(config.backfill.step, 1800000);
        assert_eq!(config.backfill.request_interval, 1000);
        assert_eq!(config.backfill.workers, 2);

        // Check that the processors were loaded
        assert!(config.processors.is_some());
        let processors = config.processors.unwrap();
        assert!(processors.processors.contains_key("custom_processor"));
        let custom_processor = &processors.processors["custom_processor"];
        assert_eq!(custom_processor.config["field1"], serde_json::json!("value1"));
        assert_eq!(custom_processor.config["field2"], serde_json::json!(42));
    }

    #[test]
    #[should_panic(expected = "Failed to read config file")]
    fn test_error_on_missing_config_file() {
        let args = CliArgs {
            config_path: "non_existent_config.toml".to_string(),
            network: Some("testnet".to_string()),
        };

        let _: Config = args.clone().into();
    }

    #[test]
    fn test_get_database_url() {
        // Test with all POSTGRES_* variables set
        std::env::set_var("POSTGRES_USER", "testuser");
        std::env::set_var("POSTGRES_PASSWORD", "testpass");
        std::env::set_var("POSTGRES_DB", "testdb");
        std::env::set_var("POSTGRES_HOST", "testhost");
        std::env::set_var("POSTGRES_PORT", "5433");

        let result = get_database_url();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "postgresql://testuser:testpass@testhost:5433/testdb");

        // Test with defaults (no HOST/PORT set)
        std::env::remove_var("POSTGRES_HOST");
        std::env::remove_var("POSTGRES_PORT");
        let result = get_database_url();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "postgresql://testuser:testpass@localhost:5432/testdb");

        // Test without required variables
        std::env::remove_var("POSTGRES_USER");
        let result = get_database_url();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("POSTGRES_USER"));

        // Cleanup
        std::env::remove_var("POSTGRES_PASSWORD");
        std::env::remove_var("POSTGRES_DB");
    }
}
