use std::sync::Arc;

use bento_trait::processor::{new_processor, DynProcessor, ProcessorTrait};
use bento_types::config::AppConfigTrait;

use crate::db::DbPool;

// Function type for processor factories
pub type ProcessorFactory = fn(Arc<DbPool>, Option<Arc<dyn AppConfigTrait>>) -> Box<dyn ProcessorTrait>;

/// Extensible processor configuration with support for custom processors
#[derive(Debug, Clone)]
pub enum ProcessorConfig {
    /// Built-in processors
    BlockProcessor,
    EventProcessor,
    TxProcessor,

    /// Custom processors with config
    Custom {
        name: String,
        factory: ProcessorFactory,
        config: Option<Arc<dyn AppConfigTrait>>,
    },
}

impl ProcessorConfig {
    pub fn name(&self) -> &str {
        match self {
            ProcessorConfig::BlockProcessor => "block",
            ProcessorConfig::EventProcessor => "event",
            ProcessorConfig::TxProcessor => "tx",
            ProcessorConfig::Custom { name, .. } => name,
        }
    }

    /// Create a new custom processor config
    pub fn custom<S: Into<String>>(
        name: S,
        factory: ProcessorFactory,
        config: Option<Arc<dyn AppConfigTrait>>,
    ) -> Self {
        Self::Custom { name: name.into(), factory, config }
    }

    /// Build a processor from this config
    pub fn build_processor(&self, db_pool: Arc<DbPool>) -> DynProcessor {
        match self {
            ProcessorConfig::BlockProcessor => {
                new_processor(crate::processors::block_processor::BlockProcessor::new(db_pool))
            }
            ProcessorConfig::EventProcessor => {
                new_processor(crate::processors::event_processor::EventProcessor::new(db_pool))
            }
            ProcessorConfig::TxProcessor => {
                new_processor(crate::processors::tx_processor::TxProcessor::new(db_pool))
            }
            ProcessorConfig::Custom { factory, config, .. } => factory(db_pool, config.clone()),
        }
    }
}
