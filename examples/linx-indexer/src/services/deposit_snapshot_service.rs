use crate::{models::MarketState, random_tx_id, repository::LendingRepository};
use anyhow::Context;
use bento_cli::load_config;
use bento_core::{Client, DbPool};
use bento_trait::stage::ContractsProvider;
use bento_types::{
    CallContractParams, CallContractResultType, network::Network,
    utils::timestamp_millis_to_naive_datetime,
};
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use std::sync::Arc;

pub struct DepositSnapshotService {
    lending_repository: LendingRepository,
    network: Network,
    client: Client,
}

impl DepositSnapshotService {
    pub fn new(db_pool: Arc<DbPool>, network: Network) -> Self {
        let client = Client::new(network.clone());
        Self { lending_repository: LendingRepository::new(db_pool), network, client }
    }

    pub async fn generate_snapshots(&self) -> anyhow::Result<()> {
        let config_path = "config.toml";
        let config = load_config(&config_path).expect("Failed to load config");
        let processor_config = config.processors.as_ref().and_then(|p| p.processors.get("lending"));
        let lending_processor_config =
            processor_config.is_some().then_some(serde_json::to_value(processor_config)?);
        println!("Lending Processor Config: {:?}", lending_processor_config);
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
            println!("Processing market: {}", market.id);
            let market_state = self.get_market_state(&market.id, &linx_address, linx_group).await?;
            println!("Market state: {:?}", market_state);
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

                // Process positions here
                for position in positions {
                    println!("Processing position: {:?}", position);
                    //TODO: Calculate deposit value and store snapshot
                    // Calculates deposit values
                    // Stores snapshots
                }

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
                tracing::error!("Failed to fetch state for market {}: {}", market_id, e);
                return Err(anyhow::anyhow!("Failed to fetch market state: {}", e));
            }
        };

        match result.result_type {
            CallContractResultType::CallContractFailed => {
                tracing::error!("Contract call failed for market {}", market_id);
                anyhow::bail!("Contract call failed for market {}", market_id);
            }
            CallContractResultType::CallContractSucceeded => {
                let returns = result.returns.ok_or_else(|| {
                    anyhow::anyhow!("No returns in contract call for market {}", market_id)
                })?;
                if returns.len() != 6 {
                    tracing::error!(
                        "Unexpected number of return values for market {}: expected 6, got {}",
                        market_id,
                        returns.len()
                    );
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

    pub async fn run_scheduler(&self) -> anyhow::Result<()> {
        todo!()
    }
}
