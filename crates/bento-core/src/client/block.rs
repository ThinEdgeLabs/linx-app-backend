use bento_trait::stage::BlockProvider;
use bento_types::{
    BlockAndEvents, BlockEntry, BlockHashesResponse, BlockHeaderEntry,
    BlocksAndEventsPerTimestampRange, BlocksPerTimestampRange, ChainInfo,
};

use anyhow::Result;
use async_trait::async_trait;
use url::Url;

use super::Client;
#[async_trait]
impl BlockProvider for Client {
    // List blocks on the given time interval.
    // GET:/blockflow/blocks?fromTs={from_ts}&toTs={to_ts}
    async fn get_blocks(&self, from_ts: u128, to_ts: u128) -> Result<BlocksPerTimestampRange> {
        let endpoint = format!("blockflow/blocks?fromTs={}&toTs={}", from_ts, to_ts);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }

    /// List blocks with events on the given time interval.
    ///
    /// # Arguments
    ///
    /// * `from_ts` - The starting timestamp for the block and event query.
    /// * `to_ts` - The ending timestamp for the block and event query.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `BlocksAndEventsPerTimestampRange` structure, or an error if the request fails.
    async fn get_blocks_and_events(
        &self,
        from_ts: u64,
        to_ts: u64,
    ) -> Result<BlocksAndEventsPerTimestampRange> {
        // Using the rich-blocks endpoint to get complete tx input data
        let endpoint = format!("blockflow/rich-blocks?fromTs={}&toTs={}", from_ts, to_ts);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;

        // Let middleware handle all retries, fail fast on deserialization
        let response = self.inner.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API returned error status: {}", response.status()));
        }

        let data = response.json::<BlocksAndEventsPerTimestampRange>().await.map_err(|e| {
            tracing::error!("Failed to deserialize response: {:?}", e);
            tracing::error!("timestamp range: {} - {}", from_ts, to_ts);
            anyhow::anyhow!("Error decoding response body: {:?}", e)
        })?;

        Ok(data)
    }

    // Get a block with hash.
    // GET:/blockflow/blocks/{block_hash}
    async fn get_block(&self, block_hash: &str) -> Result<BlockEntry> {
        let endpoint = format!("blockflow/blocks/{}", block_hash);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }

    /// Get a block with events by its hash.
    ///
    /// # Arguments
    ///
    /// * `block_hash` - The hash of the block to retrieve along with events.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `BlockAndEvents` structure, or an error if the request fails.
    async fn get_block_and_events_by_hash(&self, block_hash: &str) -> Result<BlockAndEvents> {
        let endpoint = format!("blockflow/rich-blocks/{}", block_hash);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }

    // Get block header.
    // GET:/blockflow/headers/{block_hash}
    async fn get_block_header(&self, block_hash: &str) -> Result<BlockHeaderEntry> {
        let endpoint = format!("blockflow/headers/{}", block_hash);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }

    async fn get_block_hash_by_height(
        &self,
        height: u64,
        from_group: u32,
        to_group: u32,
    ) -> Result<Vec<String>> {
        let endpoint = format!(
            "blockflow/hashes?height={}&fromGroup={}&toGroup={}",
            height, from_group, to_group
        );
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response: BlockHashesResponse = self.inner.get(url).send().await?.json().await?;
        Ok(response.headers)
    }

    async fn get_chain_info(&self, from_group: u32, to_group: u32) -> Result<ChainInfo> {
        let endpoint =
            format!("blockflow/chain-info?fromGroup={}&toGroup={}", from_group, to_group);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }
}
