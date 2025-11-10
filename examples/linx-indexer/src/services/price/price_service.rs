use anyhow::Context;
use bento_cli::load_config;
use bento_types::network::Network;
use bigdecimal::BigDecimal;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use super::linx_price_service::LinxPriceService;
use super::oracle_price_service::OraclePriceService;

#[derive(Clone)]
struct CachedPrice {
    price: BigDecimal,
    cached_at: Instant,
}

pub struct PriceService {
    oracle_service: OraclePriceService,
    linx_service: LinxPriceService,
    cache: Arc<RwLock<HashMap<String, CachedPrice>>>,
    cache_ttl: Duration,
}

impl PriceService {
    pub fn new(network: Network) -> Self {
        let config_path = "config.toml";
        let config = load_config(&config_path).expect("Failed to load config");

        let linx_api_url = config
            .price_service
            .as_ref()
            .map(|ps| ps.linx_api_url.clone())
            .expect("price_service config not found in config.toml");

        let oracle_service = OraclePriceService::new(network);
        let linx_service = LinxPriceService::new(linx_api_url);
        let cache = Arc::new(RwLock::new(HashMap::new()));
        let cache_ttl = Duration::from_secs(300); // 5 minutes

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
            Ok((price, _timestamp)) => {
                tracing::debug!("Fetched price for token {} from oracle", token_id);
                price
            }
            Err(oracle_err) => {
                // Oracle failed (token not supported or error), try Linx API
                tracing::debug!(
                    "Oracle fetch failed for token {}: {}, trying Linx API",
                    token_id,
                    oracle_err
                );
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

    /// Clear the entire cache (useful for testing or manual refresh).
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::ALPH_TOKEN_ID;

    #[tokio::test]
    #[ignore] // Requires network access and config
    async fn test_get_token_price_real_integration() {
        let service = PriceService::new(Network::Mainnet);

        // Test with ALPH (should use oracle)
        let result = service.get_token_price(ALPH_TOKEN_ID).await;
        assert!(result.is_ok());
        let price = result.unwrap();
        assert!(price > BigDecimal::from(0));
    }

    #[tokio::test]
    #[ignore] // Requires network access and config
    async fn test_cache_functionality() {
        let service = PriceService::new(Network::Mainnet);

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
