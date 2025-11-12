use std::sync::Arc;

use crate::Client;
use anyhow::Result;
use bento_trait::{processor::DynProcessor, stage::StageHandler};
use bento_types::{BlockBatch, DbPool, StageMessage};
use tokio::sync::mpsc;

use super::stage::{ProcessorStage, StorageStage};

#[allow(dead_code)]
pub struct Pipeline {
    client: Arc<Client>,
    processor: Arc<ProcessorStage>,
    storage: Arc<StorageStage>,
}

impl Pipeline {
    pub fn new(client: Arc<Client>, db_pool: Arc<DbPool>, processor: DynProcessor) -> Self {
        let processor = Arc::new(processor);
        Self {
            client,
            processor: Arc::new(ProcessorStage { processor: processor.clone() }),
            storage: Arc::new(StorageStage { db_pool, processor }),
        }
    }

    pub async fn run(&self, batches: Vec<BlockBatch>) -> Result<()> {
        let channel_capacity = 100;
        let (process_tx, process_rx) = mpsc::channel(channel_capacity);
        let (storage_tx, storage_rx) = mpsc::channel(channel_capacity);

        // Send the fetched batches to the processor
        for batch in batches {
            process_tx.send(StageMessage::Batch(batch)).await?;
        }

        drop(process_tx);

        // Spawn stage handlers
        let processor = self.processor.clone();
        let storage = self.storage.clone();

        // Processor stage
        let process_handle = tokio::spawn(async move {
            let mut rx = process_rx;

            while let Some(msg) = rx.recv().await {
                if let StageMessage::Batch(batch) = msg {
                    let blocks_count = batch.blocks.len();
                    let range = batch.range;

                    tracing::debug!(
                        "{} processor processing batch with {} blocks (range: {} to {})",
                        processor.processor.name().to_uppercase(),
                        blocks_count,
                        range.from_ts,
                        range.to_ts
                    );

                    let result = processor.handle(StageMessage::Batch(batch)).await?;

                    if let StageMessage::Processed(output) = result {
                        storage_tx.send(StageMessage::Processed(output)).await?;
                    }
                }
            }

            // Close storage channel when processor is done
            drop(storage_tx);

            Ok::<_, anyhow::Error>(())
        });

        // Storage stage
        let storage_handle = tokio::spawn(async move {
            let mut rx = storage_rx;

            while let Some(msg) = rx.recv().await {
                if let StageMessage::Processed(output) = msg {
                    storage.handle(StageMessage::Processed(output)).await?;
                }
            }

            Ok::<_, anyhow::Error>(())
        });

        let (process_result, storage_result) = tokio::join!(process_handle, storage_handle);

        process_result??;
        storage_result??;

        tracing::debug!("Pipeline execution completed successfully");
        Ok(())
    }
}
