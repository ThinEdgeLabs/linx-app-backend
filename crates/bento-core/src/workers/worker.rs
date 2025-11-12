use crate::{
    client::Client,
    config::ProcessorConfig,
    db::{new_db_pool, DbPool},
};
use anyhow::Result;
use bento_trait::stage::BlockProvider;
use bento_types::{
    network::Network,
    repository::{get_blocks_at_height, get_max_block_timestamp},
    BlockBatch, BlockRange, DEFAULT_GROUP_NUM,
};

use super::{fetch::fetch_parallel, pipeline::Pipeline};
use std::{sync::Arc, time::Duration};
use tokio::time::sleep as tokio_sleep;

#[derive(Debug, Default, Clone, Copy)]
pub struct SyncOptions {
    pub step: u64,
    pub backstep: u64,
    pub request_interval: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BackfillOptions {
    pub start_ts: Option<u64>,
    pub stop_ts: Option<u64>,
    pub request_interval: u64,
    pub step: u64,
    pub backstep: u64,
}

pub struct Worker {
    pub db_pool: Arc<DbPool>,
    pub client: Arc<Client>,
    pub processor_configs: Vec<ProcessorConfig>,
    pub db_url: String,
    pub sync_opts: Option<SyncOptions>,
    pub backfill_opts: Option<BackfillOptions>,
    pub workers: usize,
}

impl Worker {
    pub async fn new(
        processor_configs: Vec<ProcessorConfig>,
        db_url: String,
        network: Network,
        db_pool_size: Option<u32>,
        sync_opts: Option<SyncOptions>,
        backfill_opts: Option<BackfillOptions>,
        workers: usize,
    ) -> Result<Self> {
        let db_pool = new_db_pool(&db_url, db_pool_size).await?;
        Ok(Self {
            db_pool: db_pool.clone(),
            processor_configs,
            db_url,
            sync_opts,
            backfill_opts,
            client: Arc::new(Client::new(network)),
            workers,
        })
    }

    pub async fn run(&self) -> Result<()> {
        self.run_migrations().await;

        match self.backfill_opts {
            Some(opts) => {
                tracing::info!("Starting backfill with options: {:?}", opts);
                self.run_backfill(opts).await?;
            }
            None => {
                tracing::info!("Starting sync with options: {:?}", self.sync_opts);
                self.run_sync().await?;
            }
        }
        Ok(())
    }

    pub async fn run_backfill(&self, backfill_opts: BackfillOptions) -> Result<()> {
        println!("Running backfill with options: {:?}", backfill_opts);
        let stop_ts = if let Some(ts) = backfill_opts.stop_ts {
            ts
        } else {
            self.get_latest_block_timestamp_from_node(0, 0).await?
        };
        let start_ts = match backfill_opts.start_ts {
            Some(ts) => ts,
            None => {
                // Make sure we have the genesis and one more block synced
                self.sync_at_height(0).await?;
                self.sync_at_height(1).await?;
                // Take the min timestamp of the blocks at height 1
                let blocks = get_blocks_at_height(&self.db_pool, 1).await?;
                blocks.iter().map(|b| b.timestamp).min().unwrap().and_utc().timestamp_millis()
                    as u64
            }
        };

        tracing::info!(
            "Backfilling from {} to {} with step {} and request interval {}",
            start_ts,
            stop_ts,
            backfill_opts.step,
            backfill_opts.request_interval
        );

        let mut current_ts = start_ts;
        while current_ts < stop_ts {
            let chunk_end = std::cmp::min(current_ts + backfill_opts.step, stop_ts);

            self.sync_range(current_ts, chunk_end).await?;

            current_ts = chunk_end;

            let percentage = ((chunk_end - start_ts) as f64 / (stop_ts - start_ts) as f64) * 100.0;
            tracing::info!("Progress: {:.2}% of backfill range completed", percentage);

            tokio_sleep(Duration::from_millis(backfill_opts.request_interval)).await;
        }

        Ok(())
    }

    pub async fn run_sync(&self) -> Result<()> {
        let request_interval = self.sync_opts.unwrap().request_interval;
        // let step = self.sync_opts.unwrap().step;
        let backstep = self.sync_opts.unwrap().backstep;

        loop {
            tracing::info!("Syncing...");
            let latest_remote_ts = self.get_latest_block_timestamp_from_node(0, 0).await?;
            let latest_local_ts = get_max_block_timestamp(&self.db_pool)
                .await?
                .map(|ts| ts as u64)
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);
            // If we're behind by more than `backstep`, we start from now
            // otherwise the interval is too large.
            // This means a backfill is required.
            let start_ts = if latest_remote_ts - (latest_local_ts - backstep) > backstep {
                latest_remote_ts - backstep
            } else {
                latest_local_ts - backstep
            };

            self.sync_range(start_ts, latest_remote_ts).await?;

            let sleep_duration = Duration::from_millis(request_interval);
            tracing::info!(
                "Synced blocks from {} to {}, waiting {} seconds before next sync",
                start_ts,
                latest_remote_ts,
                sleep_duration.as_secs_f64()
            );
            tokio_sleep(sleep_duration).await;
        }
    }

    /// Syncs the blocks in the range [start_ts, stop_ts].
    /// This method will fetch blocks in batches and process them using the configured processors.
    async fn sync_range(&self, start_ts: u64, stop_ts: u64) -> Result<()> {
        let range = BlockRange {
            from_ts: start_ts.try_into().unwrap(),
            to_ts: stop_ts.try_into().unwrap(),
        };

        tracing::info!("Syncing blocks in range: {:?}", range);

        let batches = match fetch_parallel(self.client.clone(), range, self.workers).await {
            Ok(batches) => batches,
            Err(err) => {
                tracing::error!(
                    range = ?range,
                    error = ?err,
                    "Failed to fetch blocks, skipping range"
                );
                return Ok(()); // Continue processing other ranges
            }
        };

        if batches.is_empty() {
            tracing::warn!("No blocks found in the specified range");
            return Ok(());
        }

        self.run_pipeline(batches).await?;

        Ok(())
    }

    /// Syncs the blocks at a specific height.
    async fn sync_at_height(&self, height: u64) -> Result<()> {
        let groups = self.get_groups();

        let block_hash_futures: Vec<_> = groups
            .iter()
            .map(|(from_group, to_group)| {
                self.client.get_block_hash_by_height(height, *from_group, *to_group)
            })
            .collect();

        let block_hashes = futures::future::join_all(block_hash_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        if block_hashes.is_empty() {
            tracing::warn!("No blocks found at height {}", height);
            return Ok(());
        }

        let block_futures: Vec<_> = block_hashes
            .iter()
            .map(|hashes| self.client.get_block_and_events_by_hash(&hashes[0]))
            .collect();
        let blocks = futures::future::join_all(block_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        self.run_pipeline(vec![BlockBatch { blocks, range: BlockRange { from_ts: 0, to_ts: 0 } }])
            .await?;

        Ok(())
    }

    fn get_groups(&self) -> Vec<(u32, u32)> {
        let mut groups = Vec::new();
        for from_group in 0..DEFAULT_GROUP_NUM {
            for to_group in 0..DEFAULT_GROUP_NUM {
                groups.push((from_group as u32, to_group as u32));
            }
        }
        groups
    }

    async fn get_latest_block_timestamp_from_node(
        &self,
        chain_from: u32,
        chain_to: u32,
    ) -> Result<u64> {
        let chain_info = self.client.get_chain_info(chain_from, chain_to).await?;
        let hashes = self
            .client
            .get_block_hash_by_height(chain_info.current_height as u64, chain_from, chain_to)
            .await?;
        let block = self.client.get_block_and_events_by_hash(&hashes[0]).await?;
        Ok(block.block.timestamp as u64)
    }

    async fn run_pipeline(&self, batches: Vec<BlockBatch>) -> Result<()> {
        let tasks: Vec<_> = self
            .processor_configs
            .iter()
            .map(|processor_config| {
                let pool_clone = self.db_pool.clone();
                let client_clone = self.client.clone();
                let processor = processor_config.build_processor(pool_clone.clone());
                let processor_name = processor.name().to_string();
                let batches_clone = batches.clone();
                let pipeline = Pipeline::new(client_clone, pool_clone, processor);

                async move {
                    pipeline.run(batches_clone).await.map_err(|err| {
                        tracing::error!(
                            processor_name = processor_name,
                            error = ?err,
                            "Processor execution failed"
                        );
                        anyhow::anyhow!("Processor {} failed: {}", processor_name, err)
                    })
                }
            })
            .collect();

        // Run all processors concurrently
        let results = futures::future::join_all(tasks).await;

        // Check if any failed
        for result in results {
            result?;
        }

        Ok(())
    }

    // For the normal processor build we just use standard Diesel with the postgres
    // feature enabled (which uses libpq under the hood, hence why we named the feature
    // this way).
    #[cfg(feature = "libpq")]
    async fn run_migrations(&self) {
        use crate::db::run_pending_migrations;
        use diesel::{pg::PgConnection, Connection};

        tracing::info!("Running migrations: {:?}", self.db_url);
        let mut conn = PgConnection::establish(&self.db_url).expect("migrations failed!");
        run_pending_migrations(&mut conn);
    }
}
