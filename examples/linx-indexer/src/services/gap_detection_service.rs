use anyhow::Result;
use bento_core::DbPool;
use bento_core::workers::worker::Worker;
use diesel::QueryableByName;
use diesel::sql_types::{BigInt, Nullable};
use diesel_async::RunQueryDsl;
use serde::Serialize;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BlockGap {
    pub chain_from: i64,
    pub chain_to: i64,
    pub missing_heights: Vec<i64>,
    pub total_missing: usize,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GapDetectionReport {
    pub block_gaps: Vec<BlockGap>,
    pub total_missing_blocks: usize,
}

#[derive(QueryableByName)]
struct MissingHeight {
    #[diesel(sql_type = BigInt)]
    chain_from: i64,
    #[diesel(sql_type = BigInt)]
    chain_to: i64,
    #[diesel(sql_type = Nullable<BigInt>)]
    missing_height: Option<i64>,
}

pub struct GapDetectionService {
    db_pool: Arc<DbPool>,
}

impl GapDetectionService {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    /// Detect missing block heights per chain
    ///
    /// # Arguments
    /// * `min_height` - Optional minimum height to start gap detection from (useful for dapps deployed after genesis)
    pub async fn detect_block_gaps(&self, min_height: Option<i64>) -> Result<Vec<BlockGap>> {
        let min_height_filter = min_height.unwrap_or(0);

        tracing::info!("Starting block gap detection (min_height: {})", min_height_filter);

        let sql = r#"
            WITH gaps AS (
                SELECT
                    chain_from,
                    chain_to,
                    height,
                    LAG(height) OVER (PARTITION BY chain_from, chain_to ORDER BY height) as prev_height
                FROM blocks
                WHERE height >= $1
                ORDER BY chain_from, chain_to, height
            ),
            gap_starts AS (
                SELECT
                    chain_from,
                    chain_to,
                    prev_height + 1 as gap_start,
                    height - 1 as gap_end
                FROM gaps
                WHERE height - prev_height > 1
            ),
            expanded_gaps AS (
                SELECT
                    chain_from,
                    chain_to,
                    generate_series(gap_start, gap_end) as missing_height
                FROM gap_starts
            )
            SELECT
                chain_from,
                chain_to,
                missing_height
            FROM expanded_gaps
            WHERE missing_height >= $1
            ORDER BY chain_from, chain_to, missing_height
        "#;

        let mut conn = self.db_pool.get().await?;
        println!("Running gap detection SQL query (min_height: {})...", min_height_filter);
        let missing_heights: Vec<MissingHeight> =
            diesel::sql_query(sql).bind::<BigInt, _>(min_height_filter).load(&mut conn).await?;
        println!("Gap detection query completed. Processing results...");

        // Group by chain
        let mut gaps_by_chain: HashMap<(i64, i64), Vec<i64>> = HashMap::new();
        for mh in missing_heights {
            if let Some(height) = mh.missing_height {
                gaps_by_chain.entry((mh.chain_from, mh.chain_to)).or_default().push(height);
            }
        }

        let mut block_gaps: Vec<BlockGap> = gaps_by_chain
            .into_iter()
            .map(|((chain_from, chain_to), heights)| {
                let total_missing = heights.len();
                BlockGap { chain_from, chain_to, missing_heights: heights, total_missing }
            })
            .collect();

        // Sort by chain
        block_gaps.sort_by_key(|g| (g.chain_from, g.chain_to));

        tracing::info!("Found {} chains with missing blocks", block_gaps.len());

        Ok(block_gaps)
    }

    /// Generate a comprehensive gap detection report
    ///
    /// # Arguments
    /// * `min_height` - Optional minimum height to start gap detection from
    pub async fn generate_report(&self, min_height: Option<i64>) -> Result<GapDetectionReport> {
        tracing::info!("Generating gap detection report");

        let block_gaps = self.detect_block_gaps(min_height).await?;
        let total_missing_blocks: usize = block_gaps.iter().map(|g| g.total_missing).sum();
        println!("Total missing blocks detected: {}", total_missing_blocks);
        Ok(GapDetectionReport { block_gaps, total_missing_blocks })
    }

    /// Backfill all detected gaps
    ///
    /// # Arguments
    /// * `worker` - Worker instance to use for backfilling
    /// * `min_height` - Optional minimum height to start gap detection from
    /// * `delay_ms` - Delay in milliseconds between each height to avoid overwhelming the node (default: 100ms)
    pub async fn backfill_gaps(
        &self,
        worker: &Worker,
        min_height: Option<i64>,
        delay_ms: Option<u64>,
    ) -> Result<()> {
        use tokio::time::{Duration, sleep};

        let delay = Duration::from_millis(delay_ms.unwrap_or(100));

        tracing::info!("Starting backfill of all detected gaps");

        let gaps = self.detect_block_gaps(min_height).await?;

        // Collect all unique heights that need to be backfilled
        let mut all_heights: Vec<u64> =
            gaps.iter().flat_map(|g| g.missing_heights.iter().map(|&h| h as u64)).collect();

        // Remove duplicates and sort
        all_heights.sort_unstable();
        all_heights.dedup();

        tracing::info!(
            "Backfilling {} blocks across {} chains (delay: {}ms per height)",
            all_heights.len(),
            gaps.len(),
            delay.as_millis()
        );

        let mut success_count = 0;
        let mut failure_count = 0;

        for (idx, height) in all_heights.iter().enumerate() {
            tracing::info!("Backfilling height {} ({}/{})...", height, idx + 1, all_heights.len());

            match worker.sync_at_height(*height).await {
                Ok(_) => {
                    success_count += 1;
                    tracing::info!("✓ Successfully backfilled height {}", height);
                }
                Err(e) => {
                    failure_count += 1;
                    tracing::error!("✗ Failed to backfill height {}: {}", height, e);

                    // Log the full error chain for debugging
                    let mut source = e.source();
                    let mut depth = 1;
                    while let Some(err) = source {
                        tracing::error!("  Caused by (level {}): {}", depth, err);
                        source = err.source();
                        depth += 1;
                    }

                    // Continue with other blocks even if one fails
                }
            }

            // Add delay to avoid overwhelming the node, but skip delay after the last height
            if idx < all_heights.len() - 1 {
                sleep(delay).await;
            }
        }

        tracing::info!(
            "Backfill completed: {} successful, {} failed out of {} total",
            success_count,
            failure_count,
            all_heights.len()
        );

        if failure_count > 0 {
            tracing::warn!(
                "Some blocks failed to backfill. Re-run the command to retry failed blocks."
            );
        }

        Ok(())
    }
}
