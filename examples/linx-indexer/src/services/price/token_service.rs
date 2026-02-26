use anyhow::{Context, Result};
use async_trait::async_trait;
use bento_types::network::Network;
use bigdecimal::BigDecimal;
#[cfg(test)]
use mockall::automock;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use super::linx_price_service::{LinxPriceService, TokenInfo};
use super::oracle_price_service::OraclePriceService;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TokenServiceTrait {
    async fn get_token_price(&self, token_id: &str) -> Result<BigDecimal>;
    async fn get_token_info(&self, token_id: &str) -> Result<TokenInfo>;
    async fn get_token_decimals(&self, token_id: &str) -> Result<u8>;
}

#[derive(Clone)]
struct CachedPrice {
    price: BigDecimal,
    cached_at: Instant,
}

pub struct TokenService {
    oracle_service: OraclePriceService,
    linx_service: LinxPriceService,
    cache: Arc<RwLock<HashMap<String, CachedPrice>>>,
    cache_ttl: Duration,
}

#[cfg_attr(test, automock)]
impl TokenService {
    pub fn new(network: Network, linx_api_url: String, dia_oracle_address: String, linx_group: u32) -> Self {
        let oracle_service = OraclePriceService::new(network.clone(), dia_oracle_address, linx_group);
        let linx_service = LinxPriceService::new(linx_api_url, network);
        let cache = Arc::new(RwLock::new(HashMap::new()));
        let cache_ttl = Duration::from_secs(30); // cache prices for 30 seconds

        Self { oracle_service, linx_service, cache, cache_ttl }
    }

    /// Get price for a token by token ID.
    /// Returns the price in USD.
    ///
    /// Strategy:
    /// 1. Check cache first
    /// 2. Try oracle (if token is supported)
    /// 3. Fall back to Linx API
    /// 4. Cache the result
    pub async fn get_token_price(&self, token_id: &str) -> anyhow::Result<BigDecimal> {
        // 1. Check cache
        if let Some(cached_price) = self.get_from_cache(token_id) {
            tracing::debug!("Cache hit for token {}", token_id);
            return Ok(cached_price);
        }

        // 2. Try oracle first, then fall back to Linx API
        let price = match self.oracle_service.get_token_price(token_id).await {
            Ok(price) => {
                tracing::debug!("Fetched price for token {} from oracle", token_id);
                price
            }
            Err(oracle_err) => {
                // Oracle failed (token not supported or error), try Linx API
                tracing::debug!("Oracle fetch failed for token {}: {}, trying Linx API", token_id, oracle_err);
                self.linx_service.get_token_price(token_id).await.context(format!(
                    "Both oracle and Linx API failed for token {}. Oracle error: {}",
                    token_id, oracle_err
                ))?
            }
        };

        // 3. Cache the result
        self.set_cache(token_id, price.clone());

        Ok(price)
    }

    /// Get full token info including metadata and price.
    pub async fn get_token_info(&self, token_id: &str) -> anyhow::Result<TokenInfo> {
        self.linx_service.get_token_info(token_id).await
    }

    /// Get token decimals for amount conversion.
    pub async fn get_token_decimals(&self, token_id: &str) -> anyhow::Result<u8> {
        self.linx_service.get_token_decimals(token_id).await
    }

    /// Get a price from cache if it exists and is not expired.
    fn get_from_cache(&self, token_id: &str) -> Option<BigDecimal> {
        let cache = self.cache.read().ok()?;
        let cached = cache.get(token_id)?;

        if cached.cached_at.elapsed() < self.cache_ttl { Some(cached.price.clone()) } else { None }
    }

    /// Store a price in the cache.
    fn set_cache(&self, token_id: &str, price: BigDecimal) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(token_id.to_string(), CachedPrice { price, cached_at: Instant::now() });
        }
    }

    /// Clear the entire cache (for testing or manual refresh).
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }
}

#[async_trait]
impl TokenServiceTrait for TokenService {
    async fn get_token_price(&self, token_id: &str) -> Result<BigDecimal> {
        self.get_token_price(token_id).await
    }

    async fn get_token_info(&self, token_id: &str) -> Result<TokenInfo> {
        self.get_token_info(token_id).await
    }

    async fn get_token_decimals(&self, token_id: &str) -> Result<u8> {
        self.get_token_decimals(token_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::ALPH_TOKEN_ID;

    #[tokio::test]
    #[ignore] // Requires network access and config
    async fn test_get_token_price_real_integration() {
        let service = TokenService::new(
            Network::Mainnet,
            "https://api.linxlabs.org/tokens".to_string(),
            "test_oracle".to_string(),
            0,
        );

        // Test with ALPH (should use oracle)
        let result = service.get_token_price(ALPH_TOKEN_ID).await;
        assert!(result.is_ok());
        let price = result.unwrap();
        assert!(price > BigDecimal::from(0));
    }

    #[tokio::test]
    #[ignore] // Requires network access and config
    async fn test_cache_functionality() {
        let service = TokenService::new(
            Network::Mainnet,
            "https://api.linxlabs.org/tokens".to_string(),
            "test_oracle".to_string(),
            0,
        );

        // First call - should fetch from API
        let price1 = service.get_token_price(ALPH_TOKEN_ID).await.unwrap();

        // Second call - should use cache
        let price2 = service.get_token_price(ALPH_TOKEN_ID).await.unwrap();

        assert_eq!(price1, price2);

        // Clear cache
        service.clear_cache();

        // Third call - should fetch again
        let price3 = service.get_token_price(ALPH_TOKEN_ID).await.unwrap();
        assert!(price3 > BigDecimal::from(0));
    }
}
