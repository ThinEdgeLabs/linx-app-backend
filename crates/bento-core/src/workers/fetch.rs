use std::{sync::Arc, time::Instant};

use anyhow::Result;
use bento_trait::stage::BlockProvider;
use futures::{stream::FuturesOrdered, StreamExt};

use bento_types::{BlockAndEvents, BlockBatch, BlockRange, MAX_TIMESTAMP_RANGE};

pub async fn fetch_parallel<T: BlockProvider + 'static>(
    client: Arc<T>,
    range: BlockRange,
    num_workers: usize,
) -> Result<Vec<BlockBatch>> {
    let total_time = range.to_ts - range.from_ts;
    let chunk_size = total_time / num_workers as u64;

    tracing::debug!(
        "Starting parallel fetch with {} workers for range {}-{}",
        num_workers,
        range.from_ts,
        range.to_ts
    );

    let mut futures = FuturesOrdered::new();

    for i in 0..num_workers {
        let from = range.from_ts + (i as u64 * chunk_size);
        let to = if i == num_workers - 1 { range.to_ts } else { from + chunk_size };

        let range = BlockRange { from_ts: from, to_ts: to };

        tracing::debug!(worker_id = i, from_ts = from, to_ts = to, "Dispatching worker");

        futures.push_back(fetch_chunk(client.clone(), range));
    }

    let mut results = Vec::new();
    let mut completed_workers = 0;

    while let Some(result) = futures.next().await {
        match result {
            Ok(batch) => {
                tracing::debug!(
                    worker_id = completed_workers,
                    batch_size = batch.blocks.len(),
                    "Worker completed successfully"
                );
                results.push(batch);
            }
            Err(err) => {
                let err_ctx =
                    format!("Failed to fetch chunk (worker {}/{})", completed_workers, num_workers);

                tracing::error!(
                    error = %err,
                    worker_id = completed_workers,
                    "Worker failed"
                );

                return Err(err.context(err_ctx));
            }
        }

        completed_workers += 1;
    }

    tracing::debug!("Parallel fetch completed successfully, retrieved {} batches", results.len());

    Ok(results)
}

pub async fn fetch_chunk<T: BlockProvider + 'static>(
    client: Arc<T>,
    range: BlockRange,
) -> Result<BlockBatch> {
    if (range.to_ts - range.from_ts) > MAX_TIMESTAMP_RANGE {
        return Err(anyhow::anyhow!(
            "Timestamp range exceeds maximum limit, maximum {}, got {}",
            MAX_TIMESTAMP_RANGE,
            range.to_ts - range.from_ts,
        ));
    }

    let start = Instant::now();

    let blocks: Vec<BlockAndEvents> = client
        .get_blocks_and_events(range.from_ts, range.to_ts)
        .await?
        .blocks_and_events
        .iter()
        .flatten()
        .cloned()
        .collect();

    let elapsed = start.elapsed();

    tracing::info!(
        "Fetched {} blocks from timestamp {} to timestamp {} ({} seconds) in {:.2?}",
        blocks.clone().len(),
        range.from_ts,
        range.to_ts,
        (range.to_ts - range.from_ts) / 1_000,
        elapsed
    );
    Ok(BlockBatch { blocks, range })
}

#[allow(unused_variables)]
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bento_types::ChainInfo;
    use bento_types::*;
    use mockall::predicate::*;
    use mockall::*;
    use std::sync::Arc;

    // Create a mock for the Client that implements BlockProvider
    mock! {
        pub Client {}

        #[async_trait]
        impl BlockProvider for Client {
            async fn get_blocks(&self, from_ts: u128, to_ts: u128) -> Result<BlocksPerTimestampRange>;

            async fn get_blocks_and_events(
                &self,
                from_ts: u64,
                to_ts: u64,
            ) -> Result<BlocksAndEventsPerTimestampRange>;

            async fn get_block(&self, block_hash: &str) -> Result<BlockEntry>;

            async fn get_block_and_events_by_hash(&self, block_hash: &str) -> Result<BlockAndEvents>;

            async fn get_block_header(&self, block_hash: &str) -> Result<BlockHeaderEntry>;

            async fn get_block_hash_by_height(
                &self,
                height: u64,
                from_group: u32,
                to_group: u32,
            ) -> Result<Vec<String>>;

            async fn get_chain_info(&self, from_group: u32, to_group: u32) -> Result<ChainInfo>;
        }
    }

    #[tokio::test]
    async fn test_fetch_parallel_range_division() {
        // Setup test parameters
        let from_ts = 1000;
        let to_ts = 5000;
        let range = BlockRange { from_ts, to_ts };
        let num_workers = 4;

        // Create a mock client
        let mut mock_client = MockClient::new();

        // Set expectations for each worker's range
        // Worker 0: 1000-2000
        mock_client.expect_get_blocks_and_events().with(eq(1000), eq(2000)).times(1).returning(
            |from_ts, to_ts| {
                // Create a sample block for this range
                let sample_block = create_test_block("hash1", from_ts);
                let sample_block_and_events =
                    BlockAndEvents { block: sample_block, events: vec![] };

                Ok(BlocksAndEventsPerTimestampRange {
                    blocks_and_events: vec![vec![sample_block_and_events]],
                })
            },
        );

        // Worker 1: 2000-3000
        mock_client.expect_get_blocks_and_events().with(eq(2000), eq(3000)).times(1).returning(
            |from_ts, to_ts| {
                // Create a sample block for this range
                let sample_block = create_test_block("hash2", from_ts);
                let sample_block_and_events =
                    BlockAndEvents { block: sample_block, events: vec![] };

                Ok(BlocksAndEventsPerTimestampRange {
                    blocks_and_events: vec![vec![sample_block_and_events]],
                })
            },
        );

        // Worker 2: 3000-4000
        mock_client.expect_get_blocks_and_events().with(eq(3000), eq(4000)).times(1).returning(
            |from_ts, to_ts| {
                // Create a sample block for this range
                let sample_block = create_test_block("hash3", from_ts);
                let sample_block_and_events =
                    BlockAndEvents { block: sample_block, events: vec![] };

                Ok(BlocksAndEventsPerTimestampRange {
                    blocks_and_events: vec![vec![sample_block_and_events]],
                })
            },
        );

        // Worker 3: 4000-5000
        mock_client.expect_get_blocks_and_events().with(eq(4000), eq(5000)).times(1).returning(
            |from_ts, to_ts| {
                // Create a sample block for this range
                let sample_block = create_test_block("hash4", from_ts);
                let sample_block_and_events =
                    BlockAndEvents { block: sample_block, events: vec![] };

                Ok(BlocksAndEventsPerTimestampRange {
                    blocks_and_events: vec![vec![sample_block_and_events]],
                })
            },
        );

        let client = Arc::new(mock_client);

        // Execute the function
        let result = fetch_parallel(client, range, num_workers).await;

        // Verify results
        assert!(result.is_ok(), "Expected successful result, got {:?}", result.err());
        let batches = result.unwrap();

        // Check we have the correct number of batches
        assert_eq!(
            batches.len(),
            num_workers,
            "Expected {} batches, got {}",
            num_workers,
            batches.len()
        );

        // Verify each batch covers the expected range
        let mut covered_from = from_ts;
        let mut batch_ranges: Vec<BlockRange> = batches.iter().map(|b| b.range).collect();
        batch_ranges.sort_by_key(|r| r.from_ts);

        for (i, batch) in batch_ranges.iter().enumerate() {
            assert_eq!(
                batch.from_ts, covered_from,
                "Batch {} should start at {}, but starts at {}",
                i, covered_from, batch.from_ts
            );

            // Expected end timestamp for this batch
            let expected_to = if i == num_workers - 1 {
                range.to_ts
            } else {
                range.from_ts
                    + ((i as u64 + 1) * ((range.to_ts - range.from_ts) / num_workers as u64))
            };

            assert_eq!(
                batch.to_ts, expected_to,
                "Batch {} should end at {}, but ends at {}",
                i, expected_to, batch.to_ts
            );

            covered_from = batch.to_ts;
        }

        // Verify the entire range is covered
        assert_eq!(
            covered_from, to_ts,
            "The last batch should end at the original to_ts {}, but ended at {}",
            to_ts, covered_from
        );
    }

    #[tokio::test]
    async fn test_fetch_parallel_error_handling() {
        // Setup test parameters
        let from_ts = 1000;
        let to_ts = 5000;
        let range = BlockRange { from_ts, to_ts };
        let num_workers = 4;

        // Create a mock client
        let mut mock_client = MockClient::new();

        // Worker 0: 1000-2000
        mock_client.expect_get_blocks_and_events().with(eq(1000), eq(2000)).times(1).returning(
            |from_ts, to_ts| {
                // Create a sample block for this range
                let sample_block = create_test_block("hash1", from_ts);
                let sample_block_and_events =
                    BlockAndEvents { block: sample_block, events: vec![] };

                Ok(BlocksAndEventsPerTimestampRange {
                    blocks_and_events: vec![vec![sample_block_and_events]],
                })
            },
        );

        // Worker 1: 2000-3000 - This one fails
        mock_client
            .expect_get_blocks_and_events()
            .with(eq(2000), eq(3000))
            .times(1)
            .returning(|_, _| Err(anyhow::anyhow!("Simulated worker failure")));

        let client = Arc::new(mock_client);

        // Execute the function
        let result = fetch_parallel(client, range, num_workers).await;

        // Verify error is propagated
        assert!(result.is_err(), "Expected error, got {:?}", result);
        let err_string = format!("{:#}", result.unwrap_err());
        assert!(
            err_string.contains("Failed to fetch chunk")
                && err_string.contains("Simulated worker failure"),
            "Expected error message to contain both phrases, got: {}",
            err_string
        );
    }

    #[tokio::test]
    async fn test_fetch_chunk_max_range_limit() {
        // For other mock methods we're not testing, return default values
        let mock_client = MockClient::new();

        // We shouldn't need to set any expectations because this should fail before calling any methods

        let client = Arc::new(mock_client);
        let range = BlockRange { from_ts: 1000, to_ts: 1000 + MAX_TIMESTAMP_RANGE + 1 };

        let result = fetch_chunk(client, range).await;

        assert!(result.is_err(), "Expected error due to exceeding max timestamp range");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Timestamp range exceeds maximum limit"),
            "Expected error message to mention exceeding maximum limit, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_fetch_chunk_success() {
        let mut mock_client = MockClient::new();

        // Setup test data
        let from_ts = 1000;
        let to_ts = 2000;
        let test_block = create_test_block("test_hash", from_ts);
        let test_block_and_events = BlockAndEvents { block: test_block, events: vec![] };

        mock_client.expect_get_blocks_and_events().with(eq(from_ts), eq(to_ts)).times(1).returning(
            move |from_ts, to_ts| {
                Ok(BlocksAndEventsPerTimestampRange {
                    blocks_and_events: vec![vec![test_block_and_events.clone()]],
                })
            },
        );

        let client = Arc::new(mock_client);
        let range = BlockRange { from_ts, to_ts };

        let result = fetch_chunk(client, range).await;

        assert!(result.is_ok(), "Expected successful fetch, got {:?}", result.err());
        let batch = result.unwrap();
        assert_eq!(batch.blocks.len(), 1, "Expected 1 block, got {}", batch.blocks.len());
        assert_eq!(
            batch.range.from_ts, from_ts,
            "Expected range.from_ts to be {}, got {}",
            from_ts, batch.range.from_ts
        );
        assert_eq!(
            batch.range.to_ts, to_ts,
            "Expected range.to_ts to be {}, got {}",
            to_ts, batch.range.to_ts
        );
    }

    // Helper function to create a test block
    fn create_test_block(hash: &str, timestamp: u64) -> RichBlockEntry {
        RichBlockEntry {
            hash: hash.to_string(),
            timestamp: timestamp as i64,
            chain_from: 1,
            chain_to: 2,
            height: 1000,
            deps: vec!["dep1".to_string(), "dep2".to_string()],
            transactions: vec![],
            nonce: "test_nonce".to_string(),
            version: 1,
            dep_state_hash: "dep_hash".to_string(),
            txs_hash: "txs_hash".to_string(),
            target: "target".to_string(),
            ghost_uncles: vec![],
            parent: Some("parent_hash".to_string()),
            main_chain: Some(true),
        }
    }
}
