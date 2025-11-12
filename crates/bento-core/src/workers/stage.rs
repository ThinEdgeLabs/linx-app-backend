use std::sync::Arc;

use anyhow::Result;
use bento_trait::{processor::DynProcessor, stage::StageHandler};
use bento_types::{DbPool, StageMessage};

pub struct ProcessorStage {
    pub processor: Arc<DynProcessor>,
}

impl ProcessorStage {
    pub fn new(processor: Arc<DynProcessor>) -> Self {
        Self { processor }
    }
}

#[async_trait::async_trait]
impl StageHandler for ProcessorStage {
    async fn handle(&self, msg: StageMessage) -> Result<StageMessage> {
        match msg {
            StageMessage::Batch(batch) => {
                let output = self.processor.process_blocks(batch.blocks).await?;
                Ok(StageMessage::Processed(output))
            }
            _ => Ok(msg),
        }
    }
}

pub struct StorageStage {
    pub db_pool: Arc<DbPool>,
    pub processor: Arc<DynProcessor>,
}

impl StorageStage {
    pub fn new(db_pool: Arc<DbPool>, processor: Arc<DynProcessor>) -> Self {
        Self { db_pool, processor }
    }
}

#[async_trait::async_trait]
impl StageHandler for StorageStage {
    async fn handle(&self, msg: StageMessage) -> Result<StageMessage> {
        match msg {
            StageMessage::Processed(output) => {
                self.processor.store_output(output).await?;
                Ok(StageMessage::Complete)
            }
            _ => Ok(msg),
        }
    }
}
