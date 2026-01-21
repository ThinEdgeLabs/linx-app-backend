use std::collections::HashMap;

use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::load_config;

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

    /// The timestamp to start the backfill from
    #[arg(long = "start")]
    pub start: Option<u64>,

    /// The timestamp to stop the backfill at
    #[arg(long = "stop")]
    pub stop: Option<u64>,
}

impl From<CliArgs> for Config {
    fn from(args: CliArgs) -> Self {
        load_config(&args.config_path).expect("Failed to load config")
    }
}

impl From<BackfillArgs> for Config {
    fn from(args: BackfillArgs) -> Self {
        load_config(&args.config_path).expect("Failed to load config")
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub worker: WorkerConfig,
    pub server: ServerConfig,
    pub backfill: BackfillConfig,
    pub processors: Option<ProcessorsConfig>,
    pub price_service: Option<PriceServiceConfig>,
    pub points: Option<PointsConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub request_interval: u64,
    pub step: u64,
    pub backstep: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {}

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

#[derive(Debug, Deserialize, Serialize)]
pub struct PriceServiceConfig {
    pub linx_api_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PointsConfig {
    pub referral_percentage: f64,
    #[serde(default = "default_calculation_time")]
    pub calculation_time: String,
    #[serde(default = "default_signup_bonus")]
    pub signup_bonus: i32,
}

fn default_calculation_time() -> String {
    "01:00".to_string()
}

fn default_signup_bonus() -> i32 {
    1000  // Default 1000 points signup bonus
}

#[derive(Args)]
pub struct RunCommand {
    #[command(subcommand)]
    pub mode: RunMode,
}
