use std::{fmt::Debug, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use bento_core::{DbPool, ProcessorFactory};
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    BlockAndEvents, ContractEventByBlockHash, CustomProcessorOutput, EventField, RichBlockEntry,
    processors::ProcessorOutput,
};
use serde_json::Value;

use crate::{models::Market, repository::LendingRepository};

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, args: Option<Value>| {
        let processor = LendingProcessor::new(db_pool, args);
        Box::new(processor)
    }
}

#[derive(Debug, Clone)]
pub struct LendingProcessorOutput {
    pub markets: Vec<Market>,
}

impl CustomProcessorOutput for LendingProcessorOutput {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> {
        Box::new(self.clone())
    }
}

impl Debug for LendingProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LendingProcessor")
    }
}

#[async_trait]
impl ProcessorTrait for LendingProcessor {
    fn name(&self) -> &'static str {
        "lending"
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, bwe: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let mut markets: Vec<Market> = Vec::new();

        for block_events in &bwe {
            let block_markets = self.extract_markets(block_events);
            markets.extend(block_markets);
        }

        tracing::info!("Processed {} blocks with {} new markets", bwe.len(), markets.len());

        Ok(ProcessorOutput::Custom(Arc::new(LendingProcessorOutput { markets })))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output {
            if let Some(lending_output) = custom.as_any().downcast_ref::<LendingProcessorOutput>() {
                if !lending_output.markets.is_empty() {
                    self.lending_repository.insert_markets(&lending_output.markets).await?;
                    tracing::info!("Inserted {} new markets", lending_output.markets.len());
                }
            }
        }
        Ok(())
    }
}

pub struct LendingProcessor {
    connection_pool: Arc<DbPool>,
    linx_address: String,
    lending_repository: LendingRepository,
}

impl LendingProcessor {
    pub fn new(connection_pool: Arc<DbPool>, args: Option<Value>) -> Self {
        let lending_repository = LendingRepository::new(connection_pool.clone());
        let linx_address: String = args
            .and_then(|v| v.get("linx_address").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        Self { connection_pool, linx_address, lending_repository }
    }

    fn extract_markets(&self, block_and_events: &BlockAndEvents) -> Vec<Market> {
        block_and_events
            .events
            .iter()
            .filter_map(|event| self.parse_market_created_event(&block_and_events.block, event))
            .collect()
    }

    /// Parse MarketCreated event
    /// Event signature: MarketCreated(marketId: ByteVec, marketContractId: ByteVec, loanToken: ByteVec, collateralToken: ByteVec, oracle: ByteVec, interestRateModel: ByteVec, loanToValue: U256)
    fn parse_market_created_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
    ) -> Option<Market> {
        if event.contract_address == self.linx_address && event.event_index == 0 {
            Some(Market {
                id: self.extract_string_field(&event.fields, 0)?,
                market_contract_id: self.extract_string_field(&event.fields, 1)?,
                loan_token: self.extract_string_field(&event.fields, 2)?,
                collateral_token: self.extract_string_field(&event.fields, 3)?,
                oracle: self.extract_string_field(&event.fields, 4)?,
                irm: self.extract_string_field(&event.fields, 5)?,
                ltv: self.extract_bigdecimal_field(&event.fields, 6)?,
                created_at: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                    .unwrap_or_default()
                    .naive_utc(),
            })
        } else {
            None
        }
    }

    fn extract_string_field(&self, fields: &Vec<EventField>, index: usize) -> Option<String> {
        fields.get(index)?.value.as_str().map(|s| s.to_string())
    }

    fn extract_bigdecimal_field(
        &self,
        fields: &Vec<EventField>,
        index: usize,
    ) -> Option<bigdecimal::BigDecimal> {
        fields.get(index)?.value.as_str().and_then(|s| s.parse().ok())
    }
}
