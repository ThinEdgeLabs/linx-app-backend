use std::collections::HashMap;

use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "cli")]
#[command(about = "A CLI tool with server, worker, and backfill modes", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Run(RunCommand),
}

#[derive(Subcommand)]
pub enum RunMode {
    Server(CliArgs),
    Worker(CliArgs),
    Backfill(BackfillArgs),
}

#[derive(Args, Clone)]
pub struct CliArgs {
    /// Path to the config file
    #[arg(short, long, default_value = "config.toml")]
    pub config_path: String,

    /// The network to run the command on
    #[arg(short, long = "network", value_parser = ["devnet", "testnet", "mainnet"])]
    pub network: Option<String>,
}

#[derive(Args, Clone)]
pub struct BackfillArgs {
    /// Path to the config file
    #[arg(short, long, default_value = "config.toml")]
    pub config_path: String,

    /// The processor name to backfill for
    #[arg(short, long = "processor")]
    pub processor_name: Option<String>,

    /// The network to backfill for
    #[arg(short, long = "network", value_parser = ["devnet", "testnet", "mainnet"])]
    pub network: Option<String>,

    /// The timestamp to start the backfill from
    #[arg(long = "start")]
    pub start: Option<u64>,

    /// The timestamp to stop the backfill at
    #[arg(long = "stop")]
    pub stop: Option<u64>,
}

impl From<CliArgs> for Config {
    fn from(args: CliArgs) -> Self {
        let config_str =
            std::fs::read_to_string(args.config_path).expect("Failed to read config file");
        let mut config: Self = toml::from_str(&config_str).expect("Failed to parse config file");

        if args.network.is_some() {
            config.worker.network = args.network.clone().unwrap();
        }

        config
    }
}

impl From<BackfillArgs> for Config {
    fn from(args: BackfillArgs) -> Self {
        let config_str =
            std::fs::read_to_string(args.config_path).expect("Failed to read config file");
        let mut config: Self = toml::from_str(&config_str).expect("Failed to parse config file");

        if args.network.is_some() {
            config.worker.network = args.network.clone().unwrap();
        }

        config
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub worker: WorkerConfig,
    pub server: ServerConfig,
    pub backfill: BackfillConfig,
    pub processors: Option<ProcessorsConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub database_url: String,
    pub rpc_url: Option<String>,
    pub network: String,
    pub request_interval: u64,
    pub step: u64,
    pub backstep: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    pub port: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BackfillConfig {
    pub step: u64,
    pub backstep: u64,
    pub request_interval: u64,
    pub workers: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessorsConfig {
    #[serde(flatten)]
    pub processors: HashMap<String, ProcessorTypeConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessorTypeConfig {
    #[serde(flatten)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Args)]
pub struct RunCommand {
    #[command(subcommand)]
    pub mode: RunMode,
}
