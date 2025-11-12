use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    convert_bwe_to_tx_models, processors::ProcessorOutput, repository::insert_txs_to_db,
    BlockAndEvents,
};

use crate::{config::ProcessorConfig, db::DbPool, ProcessorFactory};
pub fn processor_factory() -> ProcessorFactory {
    |db_pool, _args: Option<serde_json::Value>| Box::new(TxProcessor::new(db_pool))
}

pub struct TxProcessor {
    connection_pool: Arc<DbPool>,
}

impl TxProcessor {
    pub fn new(connection_pool: Arc<DbPool>) -> Self {
        Self { connection_pool }
    }
}

impl Debug for TxProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "TxProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[async_trait]
impl ProcessorTrait for TxProcessor {
    fn name(&self) -> &'static str {
        ProcessorConfig::TxProcessor.name()
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, blocks: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let models = convert_bwe_to_tx_models(blocks);
        Ok(ProcessorOutput::Tx(models))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        match output {
            ProcessorOutput::Tx(models) => {
                if !models.is_empty() {
                    insert_txs_to_db(self.connection_pool.clone(), models).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
