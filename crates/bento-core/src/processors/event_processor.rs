use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    convert_bwe_to_event_models, processors::ProcessorOutput, repository::insert_events_to_db,
    BlockAndEvents,
};

use crate::{config::ProcessorConfig, db::DbPool, ProcessorFactory};

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, _args: Option<serde_json::Value>| Box::new(EventProcessor::new(db_pool))
}

pub struct EventProcessor {
    connection_pool: Arc<DbPool>,
}

impl EventProcessor {
    pub fn new(connection_pool: Arc<DbPool>) -> Self {
        Self { connection_pool }
    }
}

impl Debug for EventProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "EventProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[async_trait]
impl ProcessorTrait for EventProcessor {
    fn name(&self) -> &'static str {
        ProcessorConfig::EventProcessor.name()
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, blocks: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        // Process events and insert to db
        let models = convert_bwe_to_event_models(blocks);
        if !models.is_empty() {
            tracing::info!(
                processor_name = ?self.name(),
                count = ?models.len(),
                "Processed events"
            );
        }
        Ok(ProcessorOutput::Event(models))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        match output {
            ProcessorOutput::Event(events) => {
                if !events.is_empty() {
                    insert_events_to_db(self.connection_pool.clone(), events).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
