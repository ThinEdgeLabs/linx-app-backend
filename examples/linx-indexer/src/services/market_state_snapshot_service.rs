use crate::constants::{ALPH_TOKEN_ID, SECONDS_PER_YEAR, WAD};
use crate::jobs::PeriodicJob;
use crate::models::{Market, NewMarketStateSnapshot};
use crate::random_tx_id;
use crate::repository::LendingRepository;
use crate::services::price::token_service::TokenService;
use anyhow::{Context, Result};
use async_trait::async_trait;
use bento_core::{Client, DbPool};
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResultType, network::Network};
use bigdecimal::{BigDecimal, Zero};
use chrono::{DateTime, NaiveDateTime};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

pub struct MarketStateSnapshotService {
    lending_repository: LendingRepository,
    client: Client,
    token_service: Arc<TokenService>,
    linx_address: String,
    linx_group: u32,
}

impl MarketStateSnapshotService {
    pub fn new(
        db_pool: Arc<DbPool>,
        network: Network,
        token_service: Arc<TokenService>,
        linx_address: String,
        linx_group: u32,
    ) -> Self {
        let client = Client::new(network);
        Self { lending_repository: LendingRepository::new(db_pool), client, token_service, linx_address, linx_group }
    }

    pub async fn generate_snapshots(&self) -> Result<()> {
        let snapshot_time = chrono::Utc::now().naive_utc();
        let markets = self.lending_repository.get_all_markets().await?;
        tracing::info!("Generating market state snapshots for {} markets", markets.len());

        let mut snapshots = Vec::new();
        for market in markets {
            match self.build_snapshot(&market, snapshot_time).await {
                Ok(snap) => snapshots.push(snap),
                Err(e) => tracing::error!("Failed to build snapshot for market {}: {}", market.id, e),
            }
        }

        if !snapshots.is_empty() {
            self.lending_repository.insert_market_state_snapshots(&snapshots).await?;
            tracing::info!("Created {} market state snapshots", snapshots.len());
        }

        Ok(())
    }

    async fn build_snapshot(&self, market: &Market, snapshot_time: NaiveDateTime) -> Result<NewMarketStateSnapshot> {
        let state = fetch_market_state(&self.client, &self.linx_address, self.linx_group, &market.id).await?;

        let market_address = crate::address_from_contract_id(&market.market_contract_id);
        let contract_state =
            self.client.get_contract_state(&market_address).await.context("get_contract_state failed")?;
        // ALPH balances live on `asset.attoAlphAmount`; other tokens live in `asset.tokens`.
        let raw_collateral = if market.collateral_token == ALPH_TOKEN_ID {
            BigDecimal::from_str(&contract_state.asset.atto_alph_amount).unwrap_or_else(|_| BigDecimal::zero())
        } else {
            contract_state
                .asset
                .tokens
                .iter()
                .find(|t| t.id == market.collateral_token)
                .and_then(|t| BigDecimal::from_str(&t.amount).ok())
                .unwrap_or_else(BigDecimal::zero)
        };

        let loan_info = self.token_service.get_token_info(&market.loan_token).await?;
        let loan_price = self.token_service.get_token_price(&market.loan_token).await?;
        let coll_info = self.token_service.get_token_info(&market.collateral_token).await?;
        let coll_price = self.token_service.get_token_price(&market.collateral_token).await?;

        let total_supply_usd = loan_info.convert_to_decimal(&state.total_supply_assets) * &loan_price;
        let total_borrow_usd = loan_info.convert_to_decimal(&state.total_borrow_assets) * &loan_price;
        let total_collateral_usd = coll_info.convert_to_decimal(&raw_collateral) * &coll_price;

        let borrow_rate_per_second = self.get_borrow_rate(market, &state).await?;
        let borrow_apy = rate_per_second_to_apy(&borrow_rate_per_second);

        let epoch = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        let cum_supply_amount =
            self.lending_repository.sum_event_amounts(&market.id, "Supply", epoch, snapshot_time).await?;
        let cum_borrow_amount =
            self.lending_repository.sum_event_amounts(&market.id, "Borrow", epoch, snapshot_time).await?;

        let cum_supply_usd = loan_info.convert_to_decimal(&cum_supply_amount) * &loan_price;
        let cum_borrow_usd = loan_info.convert_to_decimal(&cum_borrow_amount) * &loan_price;

        let bad_debt_assets = self.lending_repository.sum_bad_debt_assets(&market.id).await?;
        let bad_debt_usd = loan_info.convert_to_decimal(&bad_debt_assets) * &loan_price;

        Ok(NewMarketStateSnapshot {
            market_id: market.id.clone(),
            total_supply_assets: state.total_supply_assets,
            total_supply_shares: state.total_supply_shares,
            total_borrow_assets: state.total_borrow_assets,
            total_borrow_shares: state.total_borrow_shares,
            snapshot_timestamp: snapshot_time,
            total_collateral_assets: raw_collateral,
            total_supply_usd: total_supply_usd.with_scale(2),
            total_borrow_usd: total_borrow_usd.with_scale(2),
            total_collateral_usd: total_collateral_usd.with_scale(2),
            borrow_apy: borrow_apy.with_scale(3),
            fee: state.fee.clone(),
            cumulative_supply_volume_usd: cum_supply_usd.with_scale(2),
            cumulative_borrow_volume_usd: cum_borrow_usd.with_scale(2),
            bad_debt_usd: bad_debt_usd.with_scale(2),
        })
    }

    /// Calls `borrowRateView(marketParams, marketState)` (method 2) on the market's IRM contract.
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
        let params = CallContractParams {
            tx_id: Some(random_tx_id()),
            group: self.linx_group,
            address: crate::address_from_contract_id(&market.irm),
            method_index: 2,
            args: Some(vec![market_params, market_state]),
            world_state_block_hash: None,
            interested_contracts: None,
            input_assets: None,
        };

        let result = self.client.call_contract(params).await.context("borrowRateView call failed")?;

        match result.result_type {
            CallContractResultType::CallContractFailed => anyhow::bail!(
                "borrowRateView failed for market {} (irm {}): {}",
                market.id,
                market.irm,
                result.error.as_deref().unwrap_or("unknown error")
            ),
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
impl PeriodicJob for MarketStateSnapshotService {
    fn name(&self) -> &'static str {
        "market-state-snapshots"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(3600)
    }

    async fn tick(&self) -> anyhow::Result<()> {
        self.generate_snapshots().await
    }
}

/// Raw market state fetched from the linx `getMarketState` (method 4) helper contract.
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
        method_index: 4,
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

fn bytevec_arg(value: &str) -> serde_json::Value {
    serde_json::json!({ "type": "ByteVec", "value": value })
}

fn u256_arg(value: &str) -> serde_json::Value {
    serde_json::json!({ "type": "U256", "value": value })
}

fn struct_arg(fields: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({ "type": "Array", "value": fields })
}

/// IRM `rate_per_second` (WAD-scaled) → annualized borrow APY as a plain decimal
/// (e.g. `0.0543` for 5.43%). 3-term Taylor approximation of `e^(rate·SECONDS_PER_YEAR / WAD) − 1`.
pub fn rate_per_second_to_apy(rate_per_second: &BigDecimal) -> BigDecimal {
    let wad = BigDecimal::from_str(WAD).unwrap();
    let first_term = rate_per_second * BigDecimal::from(SECONDS_PER_YEAR);
    let second_term = (&first_term * &first_term) / (BigDecimal::from(2) * &wad);
    let third_term = (&second_term * &first_term) / (BigDecimal::from(3) * &wad);
    (first_term + second_term + third_term) / wad
}

/// `supply_rate = borrow_rate × utilization × (1 − fee_normalized)`, where
/// utilization = totalBorrowAssets / totalSupplyAssets and fee_normalized = fee / WAD.
/// Used by read handlers to derive supply APY from a stored snapshot.
pub fn derive_supply_apy(
    borrow_apy: &BigDecimal,
    total_supply_assets: &BigDecimal,
    total_borrow_assets: &BigDecimal,
    fee: &BigDecimal,
) -> BigDecimal {
    if total_supply_assets.is_zero() {
        return BigDecimal::zero();
    }
    let wad = BigDecimal::from_str(WAD).unwrap();
    let utilization = total_borrow_assets / total_supply_assets;
    let fee_normalized = fee / &wad;
    let one = BigDecimal::from(1);
    borrow_apy * utilization * (one - fee_normalized)
}
