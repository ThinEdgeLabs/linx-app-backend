use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    convert_bwe_to_block_models, processors::ProcessorOutput, repository::insert_blocks_to_db,
    BlockAndEvents,
};

use crate::{config::ProcessorConfig, db::DbPool, ProcessorFactory};

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, _args: Option<serde_json::Value>| Box::new(BlockProcessor::new(db_pool))
}

pub struct BlockProcessor {
    connection_pool: Arc<DbPool>,
}

impl BlockProcessor {
    pub fn new(connection_pool: Arc<DbPool>) -> Self {
        Self { connection_pool }
    }
}

impl Debug for BlockProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "BlockProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[async_trait]
impl ProcessorTrait for BlockProcessor {
    fn name(&self) -> &'static str {
        ProcessorConfig::BlockProcessor.name()
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, blocks: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let models = convert_bwe_to_block_models(blocks);
        Ok(ProcessorOutput::Block(models))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        match output {
            ProcessorOutput::Block(blocks) => {
                if !blocks.is_empty() {
                    insert_blocks_to_db(self.connection_pool.clone(), blocks).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
