use crate::models::NewMarketStateSnapshot;
use crate::repository::LendingRepository;
use anyhow::{Context, Result};
use bento_cli::load_config;
use bento_core::{Client, DbPool};
use bento_trait::stage::ContractsProvider;
use bento_types::{network::Network, CallContractParams, CallContractResultType};
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use std::sync::Arc;

pub struct MarketStateSnapshotService {
    lending_repository: LendingRepository,
    client: Client,
    linx_address: String,
    linx_group: u32,
}

impl MarketStateSnapshotService {
    pub fn new(db_pool: Arc<DbPool>, network: Network) -> Self {
        let config_path = "config.toml";
        let config = load_config(config_path).expect("Failed to load config");
        let processor_config = config.processors.as_ref().and_then(|p| p.processors.get("lending"));
        let lending_processor_config =
            processor_config.is_some().then_some(serde_json::to_value(processor_config).ok()).flatten();

        let linx_address: String = lending_processor_config
            .as_ref()
            .and_then(|v| v.get("linx_address").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .expect("linx_address not found in config");

        let linx_group: u32 = lending_processor_config
            .and_then(|v| v.get("linx_group").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .expect("linx_group not found in config");

        let client = Client::new(network);

        Self { lending_repository: LendingRepository::new(db_pool), client, linx_address, linx_group }
    }

    /// Generate market state snapshots for all markets at current time
    pub async fn generate_snapshots(&self) -> Result<()> {
        let snapshot_time = chrono::Utc::now().naive_utc();
        let markets = self.lending_repository.get_all_markets().await?;

        tracing::info!("Generating market state snapshots for {} markets", markets.len());

        let mut snapshots = Vec::new();

        for market in markets {
            tracing::debug!("Fetching market state for market {}", market.id);

            match self.get_market_state(&market.id).await {
                Ok(market_state) => {
                    snapshots.push(NewMarketStateSnapshot {
                        market_id: market.id.clone(),
                        total_supply_assets: market_state.total_supply_assets,
                        total_supply_shares: market_state.total_supply_shares,
                        total_borrow_assets: market_state.total_borrow_assets,
                        total_borrow_shares: market_state.total_borrow_shares,
                        interest_rate: Some(market_state.interest_rate),
                        snapshot_timestamp: snapshot_time,
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to fetch market state for market {}: {}", market.id, e);
                    continue;
                }
            }
        }

        if !snapshots.is_empty() {
            self.lending_repository.insert_market_state_snapshots(&snapshots).await?;
            tracing::info!("Created {} market state snapshots", snapshots.len());
        }

        Ok(())
    }

    /// Run scheduler that generates snapshots every 5 minutes
    pub async fn run_scheduler(&self) -> Result<()> {
        let interval = tokio::time::Duration::from_secs(300); // 5 minutes
        let mut interval_timer = tokio::time::interval(interval);

        tracing::info!("Market state snapshot scheduler started (5 minute interval)");

        loop {
            interval_timer.tick().await;

            tracing::info!("Starting scheduled market state snapshot generation...");
            if let Err(e) = self.generate_snapshots().await {
                tracing::error!("Error during scheduled snapshot generation: {}", e);
            } else {
                tracing::info!("Scheduled market state snapshot generation completed successfully.");
            }
        }
    }

    /// Get current market state from the blockchain using helper contract
    async fn get_market_state(&self, market_id: &str) -> Result<MarketState> {
        let method_index = 5; // getMarketState method
        let tx_id = crate::random_tx_id();

        let params = CallContractParams {
            tx_id: Some(tx_id.clone()),
            group: self.linx_group,
            address: self.linx_address.clone(),
            method_index,
            args: Some(vec![serde_json::json!({
                "type": "ByteVec",
                "value": market_id,
            })]),
            world_state_block_hash: None,
            interested_contracts: None,
            input_assets: None,
        };

        let result = self
            .client
            .call_contract(params)
            .await
            .context("Failed to call contract")?;

        match result.result_type {
            CallContractResultType::CallContractFailed => {
                anyhow::bail!("Contract call failed for market {}", market_id);
            }
            CallContractResultType::CallContractSucceeded => {
                let returns =
                    result.returns.ok_or_else(|| anyhow::anyhow!("No returns in contract call"))?;

                if returns.len() != 6 {
                    anyhow::bail!("Expected 6 return values, got {}", returns.len());
                }

                let total_supply_assets = self.extract_bigdecimal(&returns[0])?;
                let total_supply_shares = self.extract_bigdecimal(&returns[1])?;
                let total_borrow_assets = self.extract_bigdecimal(&returns[2])?;
                let total_borrow_shares = self.extract_bigdecimal(&returns[3])?;
                let _timestamp = self.extract_bigdecimal(&returns[4])?.to_i64().unwrap_or(0);
                let _fee = self.extract_bigdecimal(&returns[5])?;

                // Calculate interest rate (simple estimate based on utilization)
                let interest_rate = if total_supply_assets.is_zero() {
                    BigDecimal::zero()
                } else {
                    &total_borrow_assets / &total_supply_assets
                };

                Ok(MarketState {
                    total_supply_assets,
                    total_supply_shares,
                    total_borrow_assets,
                    total_borrow_shares,
                    interest_rate,
                })
            }
        }
    }

    fn extract_bigdecimal(&self, value: &serde_json::Value) -> Result<BigDecimal> {
        value
            .as_object()
            .and_then(|obj| obj.get("value"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid object structure"))?
            .parse::<BigDecimal>()
            .map_err(|e| anyhow::anyhow!("Failed to parse BigDecimal: {}", e))
    }
}

#[derive(Debug, Clone)]
struct MarketState {
    total_supply_assets: BigDecimal,
    total_supply_shares: BigDecimal,
    total_borrow_assets: BigDecimal,
    total_borrow_shares: BigDecimal,
    interest_rate: BigDecimal,
}
