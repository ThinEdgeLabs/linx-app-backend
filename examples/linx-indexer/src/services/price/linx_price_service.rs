use anyhow::Context;
use bento_types::network::Network;
use bigdecimal::{BigDecimal, Num};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

use crate::constants::ALPH_TOKEN_ID;

const TEST_BTC_TOKEN_ID: &str = "0712ee40be418ed0105b1b9c1f255a5e3fa0ef40004f400f216df05eb014c600";
const TEST_USDT_TOKEN_ID: &str = "79804da1cd63c4575675b6391d956f4745591c65a30aa058ae6bd0a07ce64b00";
const TEST_ETH_TOKEN_ID: &str = "c52beb16cc053af22524d010dee4a4946340cb568c6c1cfc48201894b3cf7000";
const TEST_USDC_TOKEN_ID: &str = "26b3ade43c606f03ca3a171f3b5b61d6ccd89d4ea25393f8e34dde10ea922e00";

#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub description: String,
    #[serde(rename = "logoURI")]
    pub logo_uri: String,
    #[serde(rename = "priceUsd")]
    pub price_usd: f64,
}

impl TokenInfo {
    /// Convert raw token amount to decimal representation based on token decimals.
    /// For example, if decimals = 18 and raw_amount = 1e18, this returns 1.0
    pub fn convert_to_decimal(&self, raw_amount: &BigDecimal) -> BigDecimal {
        raw_amount / BigDecimal::from(10u64.pow(self.decimals as u32))
    }
}

/// Hardcoded testnet token metadata
fn testnet_token_info() -> HashMap<String, TokenInfo> {
    let mut map = HashMap::new();

    // ALPH
    map.insert(
        ALPH_TOKEN_ID.to_string(),
        TokenInfo {
            id: ALPH_TOKEN_ID.to_string(),
            name: "Alephium".to_string(),
            symbol: "ALPH".to_string(),
            decimals: 18,
            description: "Native Alephium token".to_string(),
            logo_uri: "".to_string(),
            price_usd: 0.0, // Price fetched from oracle
        },
    );

    // tBTC
    map.insert(
        TEST_BTC_TOKEN_ID.to_string(),
        TokenInfo {
            id: TEST_BTC_TOKEN_ID.to_string(),
            name: "tBTC".to_string(),
            symbol: "tBTC".to_string(),
            decimals: 18,
            description: "".to_string(),
            logo_uri: "https://raw.githubusercontent.com/alephium/token-list/master/logos/TBTC.png"
                .to_string(),
            price_usd: 0.0, // Price fetched from oracle
        },
    );

    // tUSDT
    map.insert(
        TEST_USDT_TOKEN_ID.to_string(),
        TokenInfo {
            id: TEST_USDT_TOKEN_ID.to_string(),
            name: "tUSDT".to_string(),
            symbol: "tUSDT".to_string(),
            decimals: 6,
            description: "".to_string(),
            logo_uri: "https://raw.githubusercontent.com/alephium/token-list/master/logos/TUSDT.png"
                .to_string(),
            price_usd: 0.0, // Price fetched from oracle
        },
    );

    // tETH
    map.insert(
        TEST_ETH_TOKEN_ID.to_string(),
        TokenInfo {
            id: TEST_ETH_TOKEN_ID.to_string(),
            name: "tETH".to_string(),
            symbol: "tETH".to_string(),
            decimals: 18,
            description: "".to_string(),
            logo_uri: "https://raw.githubusercontent.com/alephium/token-list/master/logos/TETH.png"
                .to_string(),
            price_usd: 0.0, // Price fetched from oracle
        },
    );

    // tUSDC
    map.insert(
        TEST_USDC_TOKEN_ID.to_string(),
        TokenInfo {
            id: TEST_USDC_TOKEN_ID.to_string(),
            name: "tUSDC".to_string(),
            symbol: "tUSDC".to_string(),
            decimals: 6,
            description: "".to_string(),
            logo_uri: "https://raw.githubusercontent.com/alephium/token-list/master/logos/TUSDC.png"
                .to_string(),
            price_usd: 0.0, // Price fetched from oracle
        },
    );

    map
}

pub struct LinxPriceService {
    api_url: String,
    client: Client,
    network: Network,
}

impl LinxPriceService {
    pub fn new(api_url: String, network: Network) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client");

        Self { api_url, client, network }
    }

    /// Get full token info including metadata and price.
    pub async fn get_token_info(&self, token_id: &str) -> anyhow::Result<TokenInfo> {
        let tokens = self.fetch_all_tokens().await?;

        tokens
            .get(token_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Token {} not found in Linx API", token_id))
    }

    /// Get price for a single token by its token ID.
    /// Returns the price in USD.
    pub async fn get_token_price(&self, token_id: &str) -> anyhow::Result<BigDecimal> {
        let token_info = self.get_token_info(token_id).await?;
        // Convert f64 to BigDecimal via string to avoid precision issues
        BigDecimal::from_str_radix(&token_info.price_usd.to_string(), 10)
            .context(format!("Failed to parse price for token {}", token_id))
    }

    /// Get decimals for a single token by its token ID.
    pub async fn get_token_decimals(&self, token_id: &str) -> anyhow::Result<u8> {
        let token_info = self.get_token_info(token_id).await?;
        Ok(token_info.decimals)
    }

    /// Get prices for multiple tokens at once.
    /// Returns a HashMap of token_id -> price in USD.
    pub async fn get_multiple_prices(
        &self,
        token_ids: &[String],
    ) -> anyhow::Result<HashMap<String, BigDecimal>> {
        let all_tokens = self.fetch_all_tokens().await?;

        let mut result = HashMap::new();
        for token_id in token_ids {
            if let Some(token_info) = all_tokens.get(token_id) {
                let price = BigDecimal::from_str_radix(&token_info.price_usd.to_string(), 10)
                    .context(format!("Failed to parse price for token {}", token_id))?;
                result.insert(token_id.clone(), price);
            }
        }

        Ok(result)
    }

    /// Fetch all tokens from the Linx API and return as a HashMap.
    /// For testnet, returns hardcoded token metadata.
    async fn fetch_all_tokens(&self) -> anyhow::Result<HashMap<String, TokenInfo>> {
        // Use hardcoded testnet data for testnet
        if matches!(self.network, Network::Testnet) {
            return Ok(testnet_token_info());
        }

        // Fetch from API for mainnet and custom networks
        let response = self
            .client
            .get(&self.api_url)
            .send()
            .await
            .context("Failed to fetch tokens from Linx API")?;

        if !response.status().is_success() {
            anyhow::bail!("Linx API returned error status: {}", response.status());
        }

        let tokens: Vec<TokenInfo> =
            response.json().await.context("Failed to parse Linx API response")?;

        let mut token_map = HashMap::new();
        for token in tokens {
            token_map.insert(token.id.clone(), token);
        }

        Ok(token_map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Ignore by default since it requires network access
    async fn test_get_token_price_real_api() {
        let service = LinxPriceService::new(
            "https://api.linxlabs.org/tokens".to_string(),
            Network::Mainnet,
        );

        // ALPH token ID
        let alph_token_id = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = service.get_token_price(alph_token_id).await;

        assert!(result.is_ok());
        let price = result.unwrap();
        assert!(price > BigDecimal::from(0));
    }

    #[tokio::test]
    #[ignore] // Ignore by default since it requires network access
    async fn test_get_multiple_prices_real_api() {
        let service = LinxPriceService::new(
            "https://api.linxlabs.org/tokens".to_string(),
            Network::Mainnet,
        );

        let token_ids = vec![
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(), // ALPH
        ];

        let result = service.get_multiple_prices(&token_ids).await;

        assert!(result.is_ok());
        let prices = result.unwrap();
        assert!(prices.len() > 0);
    }

    #[tokio::test]
    async fn test_testnet_token_metadata() {
        let service = LinxPriceService::new("".to_string(), Network::Testnet);

        // Test ALPH
        let alph_info = service.get_token_info(ALPH_TOKEN_ID).await;
        assert!(alph_info.is_ok());
        let alph = alph_info.unwrap();
        assert_eq!(alph.decimals, 18);
        assert_eq!(alph.symbol, "ALPH");

        // Test tBTC
        let btc_info = service.get_token_info(TEST_BTC_TOKEN_ID).await;
        assert!(btc_info.is_ok());
        let btc = btc_info.unwrap();
        assert_eq!(btc.decimals, 18);
        assert_eq!(btc.symbol, "tBTC");

        // Test tUSDT
        let usdt_info = service.get_token_info(TEST_USDT_TOKEN_ID).await;
        assert!(usdt_info.is_ok());
        let usdt = usdt_info.unwrap();
        assert_eq!(usdt.decimals, 6);
        assert_eq!(usdt.symbol, "tUSDT");
    }
}
