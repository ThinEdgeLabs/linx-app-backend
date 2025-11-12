use anyhow::Context;
use bento_cli::load_config;
use bento_core::Client;
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResultType, network::Network};
use bigdecimal::BigDecimal;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    constants::ALPH_TOKEN_ID, extract_bigdecimal_from_object, random_tx_id, string_to_hex,
};

const TEST_BTC_TOKEN_ID: &str = "0712ee40be418ed0105b1b9c1f255a5e3fa0ef40004f400f216df05eb014c600";
const TEST_USDT_TOKEN_ID: &str = "79804da1cd63c4575675b6391d956f4745591c65a30aa058ae6bd0a07ce64b00";

const DIA_PRECISION: u32 = 10u32.pow(8);

pub fn dia_token_pairs(network: &Network) -> HashMap<&'static str, &'static str> {
    match network {
        Network::Mainnet => mainnet_dia_token_pairs(),
        Network::Testnet => testnet_dia_token_pairs(),
        _ => HashMap::new(),
    }
}

fn mainnet_dia_token_pairs() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert(ALPH_TOKEN_ID, "ALPHUSD");
    map.insert("383bc735a4de6722af80546ec9eeb3cff508f2f68e97da19489ce69f3e703200", "BTC/USD"); // Wrapped BTC (AlphBridge)
    map.insert("556d9582463fe44fbd108aedc9f409f69086dc78d994b88ea6c9e65f8bf98e00", "USDT/USD"); // Tether USD (Ethereum via AlphBridge)
    map.insert("7ff5e72636f640eb2c28056df3b6879e4c86933505abebf566518ad396335700", "USDT/USD"); // Tether USD (BSC via AlphBridge)
    map.insert("722954d9067c5a5ad532746a024f2a9d7a18ed9b90e27d0a3a504962160b5600", "USDC/USD"); // USD Coin (Ethereum via AlphBridge)
    map.insert("75e1e9f91468616a371fe416430819bf5386a3e6a258864c574271a404ec8900", "USDC/USD"); // USD Coin (BSC via AlphBridge)
    map.insert("19246e8c2899bc258a1156e08466e3cdd3323da756d8a543c7fc911847b96f00", "ETH/USD"); // Wrapped Ether (AlphBridge)
    map
}

fn testnet_dia_token_pairs() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert(ALPH_TOKEN_ID, "ALPH/USD");
    map.insert(TEST_BTC_TOKEN_ID, "BTC/USD");
    map.insert(TEST_USDT_TOKEN_ID, "USDT/USD");
    map
}

pub struct OraclePriceService {
    client: Arc<dyn ContractsProvider + Send + Sync>,
    dia_oracle_address: String,
    group_index: u32,
    network: Network,
}

impl OraclePriceService {
    pub fn new(network: Network) -> Self {
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

        let client: Arc<dyn ContractsProvider + Send + Sync> =
            Arc::new(Client::new(network.clone()));
        Self { client, dia_oracle_address, group_index, network }
    }

    #[cfg(test)]
    pub fn new_with_client(
        client: Arc<dyn ContractsProvider + Send + Sync>,
        dia_oracle_address: String,
        group_index: u32,
        network: Network,
    ) -> Self {
        Self { client, dia_oracle_address, group_index, network }
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
                        .with_context(|| format!("Failed to extract timestamp for key {}", key))?,
                ))
            }
        }
    }

    /// Get price for a token by its token ID.
    /// Returns (price, timestamp) tuple if the token is supported by the oracle.
    /// Returns error if the token is not supported by the oracle.
    pub async fn get_token_price(&self, token_id: &str) -> anyhow::Result<BigDecimal> {
        // Check if token has a DIA oracle key mapping
        let token_pairs = dia_token_pairs(&self.network);
        let key = token_pairs
            .get(token_id)
            .ok_or_else(|| anyhow::anyhow!("Token {} not supported by DIA oracle", token_id))?;

        let data = self.get_dia_value(key).await?;
        Ok(data.0 / BigDecimal::from(DIA_PRECISION))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bento_types::CallContractResult;
    use mockall::*;

    mock! {
        pub TestContractCaller {}

        #[async_trait]
        impl ContractsProvider for TestContractCaller {
            async fn call_contract(&self, params: CallContractParams) -> anyhow::Result<CallContractResult>;
        }
    }

    fn create_success_response(price: &str, timestamp: &str) -> CallContractResult {
        CallContractResult {
            result_type: CallContractResultType::CallContractSucceeded,
            error: None,
            returns: Some(vec![
                serde_json::json!({
                    "type": "U256",
                    "value": price
                }),
                serde_json::json!({
                    "type": "U256",
                    "value": timestamp
                }),
            ]),
            gas_used: Some(0),
            contracts: Some(vec![]),
            tx_inputs: Some(vec![]),
            tx_outputs: Some(vec![]),
            events: None,
            debug_messages: None,
        }
    }

    fn create_failed_response() -> CallContractResult {
        CallContractResult {
            result_type: CallContractResultType::CallContractFailed,
            error: Some("Contract call failed".to_string()),
            returns: None,
            gas_used: Some(0),
            contracts: Some(vec![]),
            tx_inputs: Some(vec![]),
            tx_outputs: Some(vec![]),
            events: None,
            debug_messages: None,
        }
    }

    #[tokio::test]
    async fn test_get_token_price_success() {
        let mut mock_client = MockTestContractCaller::new();

        // Mock expects call_contract to be called and returns success
        mock_client
            .expect_call_contract()
            .times(1)
            .returning(|_| Ok(create_success_response("150000000000", "1704067200000")));

        let service = OraclePriceService::new_with_client(
            Arc::new(mock_client),
            "test_oracle_address".to_string(),
            0,
            Network::Mainnet,
        );

        let result = service.get_token_price(ALPH_TOKEN_ID).await;

        assert!(result.is_ok());
        let price = result.unwrap();
        assert_eq!(price.to_string(), "1500");
    }

    #[tokio::test]
    async fn test_get_token_price_unsupported_token() {
        let mock_client = MockTestContractCaller::new();

        let service = OraclePriceService::new_with_client(
            Arc::new(mock_client),
            "test_oracle_address".to_string(),
            0,
            Network::Mainnet,
        );

        // Unsupported token ID
        let unsupported_token = "1111111111111111111111111111111111111111111111111111111111111111";
        let result = service.get_token_price(unsupported_token).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported by DIA oracle"));
    }

    #[tokio::test]
    async fn test_get_token_price_contract_call_failed() {
        let mut mock_client = MockTestContractCaller::new();

        // Mock contract call returns failed result
        mock_client.expect_call_contract().times(1).returning(|_| Ok(create_failed_response()));

        let service = OraclePriceService::new_with_client(
            Arc::new(mock_client),
            "test_oracle_address".to_string(),
            0,
            Network::Mainnet,
        );

        let result = service.get_token_price(ALPH_TOKEN_ID).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Contract call failed"));
    }

    #[tokio::test]
    async fn test_get_token_price_network_error() {
        let mut mock_client = MockTestContractCaller::new();

        // Mock contract call returns network error
        mock_client
            .expect_call_contract()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("Network error")));

        let service = OraclePriceService::new_with_client(
            Arc::new(mock_client),
            "test_oracle_address".to_string(),
            0,
            Network::Mainnet,
        );

        let result = service.get_token_price(ALPH_TOKEN_ID).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to fetch dia price"));
    }

    #[tokio::test]
    async fn test_get_token_price_different_networks() {
        // Test testnet-specific token on testnet
        let mut mock_client = MockTestContractCaller::new();
        mock_client
            .expect_call_contract()
            .times(1)
            .returning(|_| Ok(create_success_response("50000000000", "1704067200000")));

        let testnet_service = OraclePriceService::new_with_client(
            Arc::new(mock_client),
            "test_oracle_address".to_string(),
            0,
            Network::Testnet,
        );

        let testnet_btc = "0712ee40be418ed0105b1b9c1f255a5e3fa0ef40004f400f216df05eb014c600";
        let result = testnet_service.get_token_price(testnet_btc).await;
        assert!(result.is_ok());

        // Test mainnet-only token on testnet (should fail with unsupported error)
        let mock_client2 = MockTestContractCaller::new();
        let testnet_service2 = OraclePriceService::new_with_client(
            Arc::new(mock_client2),
            "test_oracle_address".to_string(),
            0,
            Network::Testnet,
        );

        let mainnet_btc = "383bc735a4de6722af80546ec9eeb3cff508f2f68e97da19489ce69f3e703200";
        let result = testnet_service2.get_token_price(mainnet_btc).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported by DIA oracle"));
    }
}
