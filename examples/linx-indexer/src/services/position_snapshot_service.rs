use crate::constants::{VIRTUAL_ASSETS, VIRTUAL_SHARES};
use crate::models::NewPositionSnapshot;
use crate::services::price::token_service::TokenService;
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

pub struct PositionSnapshotService {
    lending_repository: LendingRepository,
    client: Client,
    token_service: TokenService,
}

impl PositionSnapshotService {
    pub fn new(db_pool: Arc<DbPool>, network: Network) -> Self {
        let client = Client::new(network.clone());
        let token_service = TokenService::new(network);
        Self { lending_repository: LendingRepository::new(db_pool), client, token_service }
    }

    pub async fn generate_snapshots(&self) -> anyhow::Result<()> {
        let config_path = "config.toml";
        let config = load_config(config_path).expect("Failed to load config");
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

        let markets = self.lending_repository.get_all_markets().await?;
        for market in markets {
            tracing::info!("Calculating position snapshots for market {}", market.id);

            // Get token info for the loan token
            let token_info = match self.token_service.get_token_info(&market.loan_token).await {
                Ok(info) => info,
                Err(e) => {
                    tracing::error!(
                        "Failed to get token info for token {} in market {}: {}",
                        market.loan_token,
                        market.id,
                        e
                    );
                    continue;
                }
            };

            // Get token price (already normalized by token service)
            let token_price = match self.token_service.get_token_price(&market.loan_token).await {
                Ok(price) => price,
                Err(e) => {
                    tracing::error!(
                        "Failed to get price for token {} in market {}: {}",
                        market.loan_token,
                        market.id,
                        e
                    );
                    continue;
                }
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

                    // Calculate raw supply amount from shares using the protocol formula
                    let raw_supply_amount = if position.supply_shares.is_zero() {
                        BigDecimal::from(0)
                    } else {
                        (position.supply_shares
                            * (&market_state.total_supply_assets + VIRTUAL_ASSETS)
                            / (&market_state.total_supply_shares + VIRTUAL_SHARES))
                            .with_scale(0)
                    };

                    // Calculate raw borrow amount from shares using the protocol formula
                    let raw_borrow_amount = if position.borrow_shares.is_zero() {
                        BigDecimal::from(0)
                    } else {
                        (position.borrow_shares
                            * (&market_state.total_borrow_assets + VIRTUAL_ASSETS)
                            / (&market_state.total_borrow_shares + VIRTUAL_SHARES))
                            .with_scale(0)
                    };

                    // Normalize the amounts using token decimals
                    let normalized_supply_amount =
                        token_info.convert_to_decimal(&raw_supply_amount);
                    let normalized_borrow_amount =
                        token_info.convert_to_decimal(&raw_borrow_amount);

                    // Calculate USD values (token_price is already normalized)
                    let supply_amount_usd = &normalized_supply_amount * &token_price;
                    let borrow_amount_usd = &normalized_borrow_amount * &token_price;

                    tracing::debug!(
                        "User {}: supply={} (${:.2}), borrow={} (${:.2})",
                        position.address,
                        normalized_supply_amount,
                        supply_amount_usd,
                        normalized_borrow_amount,
                        borrow_amount_usd
                    );

                    let position_snapshot = NewPositionSnapshot {
                        address: position.address.clone(),
                        market_id: market.id.clone(),
                        supply_amount: raw_supply_amount,
                        supply_amount_usd,
                        borrow_amount: raw_borrow_amount,
                        borrow_amount_usd,
                        timestamp: chrono::Utc::now().naive_utc(),
                    };
                    snapshots.push(position_snapshot);
                }
                self.lending_repository.insert_position_snapshots(&snapshots).await?;

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

                Ok(MarketState {
                    total_supply_assets,
                    total_supply_shares,
                    total_borrow_assets,
                    total_borrow_shares,
                    last_update,
                    fee,
                })
            }
        }
    }

    pub async fn run_scheduler(&self) -> anyhow::Result<()> {
        let interval = tokio::time::Duration::from_secs(300); // 5 minutes
        let mut interval_timer = tokio::time::interval(interval);

        loop {
            interval_timer.tick().await;
            tracing::info!("Starting scheduled position snapshot generation...");
            if let Err(e) = self.generate_snapshots().await {
                tracing::error!("Error during scheduled snapshot generation: {}", e);
            } else {
                tracing::info!("Scheduled position snapshot generation completed successfully.");
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
