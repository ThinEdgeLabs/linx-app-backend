use crate::jobs::PeriodicJob;
use crate::models::NewMarketStateSnapshot;
use crate::random_tx_id;
use crate::repository::LendingRepository;
use anyhow::{Context, Result};
use async_trait::async_trait;
use bento_core::{Client, DbPool};
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResultType, network::Network};
use bigdecimal::BigDecimal;
use std::sync::Arc;
use std::time::Duration;

pub struct MarketStateSnapshotService {
    lending_repository: LendingRepository,
    client: Client,
    linx_address: String,
    linx_group: u32,
}

impl MarketStateSnapshotService {
    pub fn new(db_pool: Arc<DbPool>, network: Network, linx_address: String, linx_group: u32) -> Self {
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

            match fetch_market_state(&self.client, &self.linx_address, self.linx_group, &market.id).await {
                Ok(state) => {
                    snapshots.push(NewMarketStateSnapshot {
                        market_id: market.id.clone(),
                        total_supply_assets: state.total_supply_assets,
                        total_supply_shares: state.total_supply_shares,
                        total_borrow_assets: state.total_borrow_assets,
                        total_borrow_shares: state.total_borrow_shares,
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
}

#[async_trait]
impl PeriodicJob for MarketStateSnapshotService {
    fn name(&self) -> &'static str {
        "market-state-snapshots"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(1800)
    }

    async fn tick(&self) -> anyhow::Result<()> {
        self.generate_snapshots().await
    }
}

/// Raw market state fetched from the linx `getMarketState` (method 5) helper contract.
/// All fields are preserved at their on-chain U256 scale so callers can forward them
/// to other contracts (e.g. the IRM's `borrowRateView`) without re-scaling.
#[derive(Debug, Clone)]
pub struct RawMarketState {
    pub total_supply_assets: BigDecimal,
    pub total_supply_shares: BigDecimal,
    pub total_borrow_assets: BigDecimal,
    pub total_borrow_shares: BigDecimal,
    pub last_update: BigDecimal,
    pub fee: BigDecimal,
}

pub async fn fetch_market_state(
    client: &Client,
    linx_address: &str,
    linx_group: u32,
    market_id: &str,
) -> Result<RawMarketState> {
    let params = CallContractParams {
        tx_id: Some(random_tx_id()),
        group: linx_group,
        address: linx_address.to_string(),
        method_index: 4, // getMarketState
        args: Some(vec![serde_json::json!({ "type": "ByteVec", "value": market_id })]),
        world_state_block_hash: None,
        interested_contracts: None,
        input_assets: None,
    };

    let result = client.call_contract(params).await.context("getMarketState call failed")?;

    match result.result_type {
        CallContractResultType::CallContractFailed => {
            anyhow::bail!("getMarketState failed for market {}", market_id);
        }
        CallContractResultType::CallContractSucceeded => {}
    }

    let returns = result.returns.ok_or_else(|| anyhow::anyhow!("getMarketState returned no values"))?;
    if returns.len() != 6 {
        anyhow::bail!("expected 6 returns from getMarketState, got {}", returns.len());
    }

    Ok(RawMarketState {
        total_supply_assets: extract_bigdecimal(&returns[0])?,
        total_supply_shares: extract_bigdecimal(&returns[1])?,
        total_borrow_assets: extract_bigdecimal(&returns[2])?,
        total_borrow_shares: extract_bigdecimal(&returns[3])?,
        last_update: extract_bigdecimal(&returns[4])?,
        fee: extract_bigdecimal(&returns[5])?,
    })
}

fn extract_bigdecimal(value: &serde_json::Value) -> Result<BigDecimal> {
    value
        .as_object()
        .and_then(|obj| obj.get("value"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid contract return structure: {}", value))?
        .parse::<BigDecimal>()
        .map_err(|e| anyhow::anyhow!("Failed to parse BigDecimal: {}", e))
}
