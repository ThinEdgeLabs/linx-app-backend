use crate::constants::{VIRTUAL_ASSETS, VIRTUAL_SHARES};
use crate::models::NewDepositSnapshot;
use crate::services::oracle_price_service::{OraclePriceService, dia_token_pairs};
use crate::{models::MarketState, random_tx_id, repository::LendingRepository};
use anyhow::Context;
use bento_cli::load_config;
use bento_core::{Client, DbPool};
use bento_trait::stage::ContractsProvider;
use bento_types::{
    CallContractParams, CallContractResultType, network::Network,
    utils::timestamp_millis_to_naive_datetime,
};
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use std::sync::Arc;

pub struct DepositSnapshotService {
    lending_repository: LendingRepository,
    client: Client,
    price_service: OraclePriceService,
    network: Network,
}

impl DepositSnapshotService {
    pub fn new(db_pool: Arc<DbPool>, network: Network) -> Self {
        let client = Client::new(network.clone());
        let price_service = OraclePriceService::new(network.clone());
        Self { lending_repository: LendingRepository::new(db_pool), client, price_service, network }
    }

    pub async fn generate_snapshots(&self) -> anyhow::Result<()> {
        let config_path = "config.toml";
        let config = load_config(&config_path).expect("Failed to load config");
        let processor_config = config.processors.as_ref().and_then(|p| p.processors.get("lending"));
        let lending_processor_config =
            processor_config.is_some().then_some(serde_json::to_value(processor_config)?);
        let linx_address: String = lending_processor_config
            .as_ref()
            .and_then(|v| v.get("linx_address").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap();
        let linx_group: u32 = lending_processor_config
            .and_then(|v| v.get("linx_group").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap();

        let token_pairs = dia_token_pairs(&self.network);
        let mut oracle_prices: std::collections::HashMap<&str, BigDecimal> =
            std::collections::HashMap::new();

        let markets = self.lending_repository.get_all_markets().await?;
        for market in markets {
            tracing::info!("Calculating deposit snapshots for market {}", market.id);

            // Get the oracle token pair key for the loan token
            let key = match token_pairs.get(&market.loan_token.as_str()) {
                Some(k) => *k,
                None => {
                    tracing::warn!(
                        "No DIA token pair found for loan token {}, skipping market {}",
                        market.loan_token,
                        market.id
                    );
                    continue;
                }
            };

            // Fetch the oracle price for the loan token and cache it
            let loan_token_price = match oracle_prices.get(market.loan_token.as_str()) {
                Some(p) => p.clone(),
                None => match self.price_service.get_dia_value(key).await {
                    Ok((p, _)) => {
                        oracle_prices.insert(key, p.clone());
                        p
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to fetch DIA price for token {} in market {}: {}",
                            market.loan_token,
                            market.id,
                            e
                        );
                        continue;
                    }
                },
            };

            let market_state = match self
                .get_market_state(&market.id, &linx_address, linx_group)
                .await
            {
                Ok(state) => state,
                Err(e) => {
                    tracing::error!("Failed to fetch market state for market {}: {}", market.id, e);
                    continue;
                }
            };

            let mut page = 1;
            let page_size = 100;
            loop {
                let positions = self
                    .lending_repository
                    .get_positions(Some(market.id.clone()), None, page, page_size)
                    .await?;

                if positions.is_empty() {
                    break;
                }

                let mut snapshots = Vec::new();
                for position in positions {
                    tracing::debug!("Processing position for user {}", position.address);
                    let amount = if position.supply_shares.is_zero() {
                        BigDecimal::from(0)
                    } else {
                        (position.supply_shares
                            * (&market_state.total_supply_assets + VIRTUAL_ASSETS)
                            / (&market_state.total_supply_shares + VIRTUAL_SHARES))
                            .with_scale(0)
                    };
                    tracing::debug!("Calculated amount for user {}: {}", position.address, amount);
                    //TODO: Handle normalization factor for tokens with decimals
                    // DIA price has 8 decimals
                    // Loan token might have different decimals, need to adjust accordingly
                    let amount_usd = &amount
                        * oracle_prices
                            .get(market.loan_token.as_str())
                            .unwrap_or(&BigDecimal::zero());
                    let deposit_snapshot = NewDepositSnapshot {
                        address: position.address.clone(),
                        market_id: market.id.clone(),
                        amount,
                        amount_usd,
                        timestamp: chrono::Utc::now().naive_utc(),
                    };
                    snapshots.push(deposit_snapshot);
                }
                self.lending_repository.insert_deposit_snapshots(&snapshots).await?;

                page += 1;
            }
        }
        Ok(())
    }

    async fn get_market_state(
        &self,
        market_id: &str,
        linx_address: &str,
        linx_group: u32,
    ) -> anyhow::Result<MarketState> {
        let method_index = 5;
        let tx_id = random_tx_id();
        let params = CallContractParams {
            tx_id: Some(tx_id.clone()),
            group: linx_group,
            address: linx_address.to_string(),
            method_index,
            args: Some(vec![serde_json::json!({
                "type": "ByteVec",
                "value": market_id,
            })]),
            world_state_block_hash: None,
            interested_contracts: None,
            input_assets: None,
        };

        let result = match self.client.call_contract(params).await {
            Ok(res) => res,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to fetch market state: {}", e));
            }
        };

        match result.result_type {
            CallContractResultType::CallContractFailed => {
                anyhow::bail!("Contract call failed for market {}", market_id);
            }
            CallContractResultType::CallContractSucceeded => {
                let returns = result.returns.ok_or_else(|| {
                    anyhow::anyhow!("No returns in contract call for market {}", market_id)
                })?;
                if returns.len() != 6 {
                    anyhow::bail!(
                        "Expected 6 return values for market {}, got {}",
                        market_id,
                        returns.len()
                    );
                }
                let total_supply_assets =
                    self.extract_bigdecimal_from_object(&returns[0]).with_context(|| {
                        format!("Failed to extract total_supply_assets for market {}", market_id)
                    })?;

                let total_supply_shares =
                    self.extract_bigdecimal_from_object(&returns[1]).with_context(|| {
                        format!("Failed to extract total_supply_shares for market {}", market_id)
                    })?;

                let total_borrow_assets =
                    self.extract_bigdecimal_from_object(&returns[2]).with_context(|| {
                        format!("Failed to extract total_borrow_assets for market {}", market_id)
                    })?;

                let total_borrow_shares =
                    self.extract_bigdecimal_from_object(&returns[3]).with_context(|| {
                        format!("Failed to extract total_borrow_shares for market {}", market_id)
                    })?;

                let timestamp = self
                    .extract_bigdecimal_from_object(&returns[4])
                    .with_context(|| {
                        format!("Failed to extract timestamp for market {}", market_id)
                    })?
                    .to_i64()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Failed to convert timestamp to i64 for market {}",
                            market_id
                        )
                    })?;
                let last_update = timestamp_millis_to_naive_datetime(timestamp);
                let fee = self.extract_bigdecimal_from_object(&returns[5]).unwrap_or_default();

                return Ok(MarketState {
                    total_supply_assets,
                    total_supply_shares,
                    total_borrow_assets,
                    total_borrow_shares,
                    last_update,
                    fee,
                });
            }
        }
    }

    pub async fn run_scheduler(&self) -> anyhow::Result<()> {
        let interval = tokio::time::Duration::from_secs(300); // 5 minutes
        let mut interval_timer = tokio::time::interval(interval);

        loop {
            interval_timer.tick().await;
            tracing::info!("Starting scheduled deposit snapshot generation...");
            if let Err(e) = self.generate_snapshots().await {
                tracing::error!("Error during scheduled snapshot generation: {}", e);
            } else {
                tracing::info!("Scheduled deposit snapshot generation completed successfully.");
            }
        }
    }

    fn extract_bigdecimal_from_object(
        &self,
        value: &serde_json::Value,
    ) -> anyhow::Result<BigDecimal> {
        value
            .as_object()
            .and_then(|obj| obj.get("value"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid object structure"))?
            .parse::<BigDecimal>()
            .map_err(|e| anyhow::anyhow!("Failed to parse BigDecimal: {}", e))
    }
}
