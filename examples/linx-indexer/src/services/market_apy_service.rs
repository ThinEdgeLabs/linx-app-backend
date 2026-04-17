use crate::jobs::PeriodicJob;
use crate::models::{Market, NewMarketApySnapshot};
use crate::random_tx_id;
use crate::repository::LendingRepository;
use crate::services::market_state_snapshot_service::{RawMarketState, fetch_market_state};
use anyhow::{Context, Result};
use async_trait::async_trait;
use bento_core::{Client, DbPool};
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResultType, network::Network};
use bigdecimal::{BigDecimal, Zero};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

const SECONDS_PER_YEAR: u64 = 31_536_000;
/// WAD = 1e18: the fixed-point scale the Ralph IRM uses for both rates and the fee field.
const WAD: &str = "1000000000000000000";

pub struct MarketApyService {
    lending_repository: LendingRepository,
    client: Client,
    linx_address: String,
    linx_group: u32,
}

impl MarketApyService {
    pub fn new(db_pool: Arc<DbPool>, network: Network, linx_address: String, linx_group: u32) -> Self {
        let client = Client::new(network);
        Self { lending_repository: LendingRepository::new(db_pool), client, linx_address, linx_group }
    }

    pub async fn generate_snapshots(&self) -> Result<()> {
        let snapshot_time = chrono::Utc::now().naive_utc();
        let markets = self.lending_repository.get_all_markets().await?;
        tracing::info!("Generating APY snapshots for {} markets", markets.len());

        let mut snapshots = Vec::new();
        for market in markets {
            let state = match fetch_market_state(&self.client, &self.linx_address, self.linx_group, &market.id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to fetch market state for {}: {}", market.id, e);
                    continue;
                }
            };

            let borrow_rate_per_second = match self.get_borrow_rate(&market, &state).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Failed to fetch borrow rate for {}: {}", market.id, e);
                    continue;
                }
            };

            let borrow_apy = rate_per_second_to_apy(&borrow_rate_per_second);
            let supply_apy = derive_supply_apy(&borrow_apy, &state);

            snapshots.push(NewMarketApySnapshot {
                market_id: market.id.clone(),
                borrow_rate: borrow_apy,
                supply_rate: supply_apy,
                snapshot_timestamp: snapshot_time,
            });
        }

        if !snapshots.is_empty() {
            self.lending_repository.insert_market_apy_snapshots(&snapshots).await?;
            tracing::info!("Inserted {} APY snapshots", snapshots.len());
        }

        Ok(())
    }

    /// Calls `borrowRateView(marketParams, marketState)` (method 3) on the market's IRM contract.
    /// Each Ralph struct arg is encoded as a JSON array of its flattened fields.
    async fn get_borrow_rate(&self, market: &Market, state: &RawMarketState) -> Result<BigDecimal> {
        let market_params = struct_arg(vec![
            bytevec_arg(&market.loan_token),
            bytevec_arg(&market.collateral_token),
            bytevec_arg(&market.oracle),
            bytevec_arg(&market.irm),
            u256_arg(&market.ltv.to_string()),
        ]);
        let market_state = struct_arg(vec![
            u256_arg(&state.total_supply_assets.to_string()),
            u256_arg(&state.total_supply_shares.to_string()),
            u256_arg(&state.total_borrow_assets.to_string()),
            u256_arg(&state.total_borrow_shares.to_string()),
            u256_arg(&state.last_update.to_string()),
            u256_arg(&state.fee.to_string()),
        ]);
        let args = vec![market_params, market_state];
        let params = CallContractParams {
            tx_id: Some(random_tx_id()),
            group: self.linx_group,
            address: crate::address_from_contract_id(&market.irm),
            method_index: 2, // borrowRateView
            args: Some(args),
            world_state_block_hash: None,
            interested_contracts: None,
            input_assets: None,
        };

        let result = self.client.call_contract(params).await.context("borrowRateView call failed")?;

        match result.result_type {
            CallContractResultType::CallContractFailed => {
                anyhow::bail!(
                    "borrowRateView failed for market {} (irm {}): {}",
                    market.id,
                    market.irm,
                    result.error.as_deref().unwrap_or("unknown error")
                )
            }
            CallContractResultType::CallContractSucceeded => {}
        }

        let returns = result.returns.ok_or_else(|| anyhow::anyhow!("no returns from borrowRateView"))?;
        if returns.is_empty() {
            anyhow::bail!("borrowRateView returned no values for market {}", market.id);
        }

        let raw = returns[0]
            .as_object()
            .and_then(|o| o.get("value"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("invalid borrowRateView return: {}", returns[0]))?;
        BigDecimal::from_str(raw).map_err(|e| anyhow::anyhow!("borrow rate not a decimal: {}", e))
    }
}

#[async_trait]
impl PeriodicJob for MarketApyService {
    fn name(&self) -> &'static str {
        "market-apy-snapshots"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(1800)
    }

    async fn tick(&self) -> anyhow::Result<()> {
        self.generate_snapshots().await
    }
}

fn bytevec_arg(value: &str) -> serde_json::Value {
    serde_json::json!({ "type": "ByteVec", "value": value })
}

fn u256_arg(value: &str) -> serde_json::Value {
    serde_json::json!({ "type": "U256", "value": value })
}

fn struct_arg(fields: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({ "type": "Array", "value": fields })
}

/// IRM `rate_per_second` (WAD-scaled) → annualized APY as a plain decimal (e.g. `0.0543` for 5.43%).
fn rate_per_second_to_apy(rate_per_second: &BigDecimal) -> BigDecimal {
    let wad = BigDecimal::from_str(WAD).unwrap();
    (rate_per_second * BigDecimal::from(SECONDS_PER_YEAR)) / wad
}

/// `supply_rate = borrow_rate × utilization × (1 − fee_normalized)`, where
/// utilization = totalBorrowAssets / totalSupplyAssets and fee_normalized = fee / WAD.
fn derive_supply_apy(borrow_apy: &BigDecimal, state: &RawMarketState) -> BigDecimal {
    if state.total_supply_assets.is_zero() {
        return BigDecimal::zero();
    }
    let wad = BigDecimal::from_str(WAD).unwrap();
    let utilization = &state.total_borrow_assets / &state.total_supply_assets;
    let fee_normalized = &state.fee / &wad;
    let one = BigDecimal::from(1);
    borrow_apy * utilization * (one - fee_normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(supply: &str, borrow: &str, fee: &str) -> RawMarketState {
        RawMarketState {
            total_supply_assets: BigDecimal::from_str(supply).unwrap(),
            total_supply_shares: BigDecimal::from_str(supply).unwrap(),
            total_borrow_assets: BigDecimal::from_str(borrow).unwrap(),
            total_borrow_shares: BigDecimal::from_str(borrow).unwrap(),
            last_update: BigDecimal::zero(),
            fee: BigDecimal::from_str(fee).unwrap(),
        }
    }

    #[test]
    fn rate_per_second_to_apy_matches_expected_5_percent() {
        // 0.05 / SECONDS_PER_YEAR ≈ 1.585489599188e-9. No integer rate-per-second at WAD scale
        // lands exactly on 0.05 APY, so compare with a tight tolerance instead.
        let raw = BigDecimal::from_str("1585489599").unwrap();
        let apy = rate_per_second_to_apy(&raw);
        let expected = BigDecimal::from_str("0.05").unwrap();
        let tolerance = BigDecimal::from_str("0.000001").unwrap();
        assert!((&apy - &expected).abs() < tolerance, "expected ≈0.05, got {apy}");
    }

    #[test]
    fn rate_per_second_to_apy_zero() {
        assert_eq!(rate_per_second_to_apy(&BigDecimal::zero()), BigDecimal::zero());
    }

    #[test]
    fn derive_supply_apy_zero_supply_returns_zero() {
        let s = state("0", "0", "0");
        assert_eq!(derive_supply_apy(&BigDecimal::from_str("0.05").unwrap(), &s), BigDecimal::zero());
    }

    #[test]
    fn derive_supply_apy_basic_math() {
        // 50% utilization, no fee → supply = borrow × 0.5
        let s = state("1000", "500", "0");
        let supply = derive_supply_apy(&BigDecimal::from_str("0.10").unwrap(), &s).with_scale(4);
        assert_eq!(supply, BigDecimal::from_str("0.0500").unwrap());
    }

    #[test]
    fn derive_supply_apy_with_fee() {
        // 100% utilization, 10% fee (0.1 × 1e18) → supply = borrow × 1 × 0.9
        let s = state("1000", "1000", "100000000000000000");
        let supply = derive_supply_apy(&BigDecimal::from_str("0.10").unwrap(), &s).with_scale(4);
        assert_eq!(supply, BigDecimal::from_str("0.0900").unwrap());
    }
}
