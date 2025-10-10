use anyhow::Context;
use bento_cli::load_config;
use bento_core::Client;
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResultType, network::Network};
use bigdecimal::BigDecimal;

use crate::{extract_bigdecimal_from_object, random_tx_id, string_to_hex};

pub struct OraclePriceService {
    client: Client,
    dia_oracle_address: String,
    group_index: u32,
}

impl OraclePriceService {
    pub fn new(network: Network) -> Self {
        let client = Client::new(network.clone());
        let config_path = "config.toml";
        let config = load_config(&config_path).expect("Failed to load config");
        let processor_config = config.processors.as_ref().and_then(|p| p.processors.get("lending"));
        let lending_processor_config =
            processor_config.is_some().then_some(serde_json::to_value(processor_config).unwrap());

        let dia_oracle_address: String = lending_processor_config
            .as_ref()
            .and_then(|v| v.get("dia_oracle_address").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap();
        let group_index: u32 = lending_processor_config
            .and_then(|v| v.get("linx_group").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap();

        Self { client, dia_oracle_address, group_index }
    }

    pub async fn get_dia_value(&self, key: &str) -> anyhow::Result<(BigDecimal, BigDecimal)> {
        let method_index = 0;
        let tx_id = random_tx_id();
        let params = CallContractParams {
            tx_id: Some(tx_id.clone()),
            group: self.group_index,
            address: self.dia_oracle_address.to_string(),
            method_index,
            args: Some(vec![serde_json::json!({
              "type": "ByteVec",
              "value": string_to_hex(key),
            })]),
            world_state_block_hash: None,
            interested_contracts: None,
            input_assets: None,
        };

        let result = match self.client.call_contract(params).await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Failed to fetch dia price for key {}: {}", key, e);
                return Err(anyhow::anyhow!("Failed to fetch dia price: {}", e));
            }
        };

        match result.result_type {
            CallContractResultType::CallContractFailed => {
                tracing::error!("Contract call failed for key {}", key);
                anyhow::bail!("Contract call failed for key {}", key);
            }
            CallContractResultType::CallContractSucceeded => {
                let returns = result.returns.ok_or_else(|| {
                    anyhow::anyhow!("No returns in contract call for key {}", key)
                })?;
                if returns.len() != 2 {
                    tracing::error!(
                        "Unexpected number of return values for key {}: expected 2, got {}",
                        key,
                        returns.len()
                    );
                    anyhow::bail!(
                        "Expected 2 return values for key {}, got {}",
                        key,
                        returns.len()
                    );
                }
                Ok((
                    extract_bigdecimal_from_object(&returns[0], "value")
                        .with_context(|| format!("Failed to extract price for key {}", key))?,
                    extract_bigdecimal_from_object(&returns[1], "value")
                        .with_context(|| format!("Failed to extract confidence for key {}", key))?,
                ))
            }
        }
    }
}
