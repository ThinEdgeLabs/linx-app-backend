use anyhow::Context;
use bigdecimal::{BigDecimal, Num};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct TokenInfo {
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    symbol: String,
    #[allow(dead_code)]
    decimals: u8,
    #[allow(dead_code)]
    description: String,
    #[serde(rename = "logoURI")]
    #[allow(dead_code)]
    logo_uri: String,
    #[serde(rename = "priceUsd")]
    price_usd: f64,
}

pub struct LinxPriceService {
    api_url: String,
    client: Client,
}

impl LinxPriceService {
    pub fn new(api_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client");

        Self { api_url, client }
    }

    /// Get price for a single token by its token ID.
    /// Returns the price in USD.
    pub async fn get_token_price(&self, token_id: &str) -> anyhow::Result<BigDecimal> {
        let tokens = self.fetch_all_tokens().await?;

        tokens
            .get(token_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Token {} not found in Linx price API", token_id))
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
            if let Some(price) = all_tokens.get(token_id) {
                result.insert(token_id.clone(), price.clone());
            }
        }

        Ok(result)
    }

    /// Fetch all tokens from the Linx API and return as a HashMap.
    async fn fetch_all_tokens(&self) -> anyhow::Result<HashMap<String, BigDecimal>> {
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
            // Convert f64 to BigDecimal via string to avoid precision issues
            let price = BigDecimal::from_str_radix(&token.price_usd.to_string(), 10)
                .context(format!("Failed to parse price for token {}", token.id))?;
            token_map.insert(token.id, price);
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
        let service = LinxPriceService::new("https://api.linxlabs.org/tokens".to_string());

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
        let service = LinxPriceService::new("https://api.linxlabs.org/tokens".to_string());

        let token_ids = vec![
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(), // ALPH
        ];

        let result = service.get_multiple_prices(&token_ids).await;

        assert!(result.is_ok());
        let prices = result.unwrap();
        assert!(prices.len() > 0);
    }

    #[tokio::test]
    async fn test_get_token_price_not_found() {
        // This test would need a mock server to avoid network calls
        // For now, we'll skip it and rely on integration tests
    }
}
