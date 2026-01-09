use anyhow::Result;
use bento_types::config::AppConfigTrait;
use std::collections::HashSet;

/// Application-level configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub gas_payer_addresses: HashSet<String>,
    pub linx_address: String,
    pub linx_group: u32,
    pub dia_oracle_address: String,
    pub linx_api_url: String,
}

impl AppConfigTrait for AppConfig {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl AppConfig {
    /// Load configuration from environment variables
    /// This should be called at application startup to fail-fast if config is missing
    pub fn from_env() -> Result<Self> {
        let gas_payer_addresses: HashSet<String> = std::env::var("GAS_PAYER_ADDRESSES")
            .ok()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();

        let linx_address = std::env::var("LINX_ADDRESS")
            .map_err(|_| anyhow::anyhow!("LINX_ADDRESS environment variable not set"))?;

        let linx_group: u32 = std::env::var("LINX_GROUP")
            .map_err(|_| anyhow::anyhow!("LINX_GROUP environment variable not set"))?
            .parse()
            .map_err(|_| anyhow::anyhow!("LINX_GROUP must be a valid u32"))?;

        let dia_oracle_address = std::env::var("DIA_ORACLE_ADDRESS")
            .map_err(|_| anyhow::anyhow!("DIA_ORACLE_ADDRESS environment variable not set"))?;

        let linx_api_url = std::env::var("LINX_API_URL")
            .map_err(|_| anyhow::anyhow!("LINX_API_URL environment variable not set"))?;

        Ok(Self { gas_payer_addresses, linx_address, linx_group, dia_oracle_address, linx_api_url })
    }
}
