use std::{collections::HashMap, fmt::Debug, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use bento_core::{DbPool, ProcessorFactory};
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    BlockAndEvents, ContractEventByBlockHash, CustomProcessorOutput, EventField, RichBlockEntry,
    processors::ProcessorOutput,
};
use serde_json::Value;

use crate::{
    models::{Market, NewLendingEvent},
    repository::LendingRepository,
};

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, args: Option<Value>| {
        let processor = LendingProcessor::new(db_pool, args);
        Box::new(processor)
    }
}

#[derive(Debug, Clone)]
pub struct LendingProcessorOutput {
    pub markets: Vec<Market>,
    pub events: Vec<NewLendingEvent>,
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
        let mut new_markets: Vec<Market> = Vec::new();
        let mut events: Vec<NewLendingEvent> = Vec::new();

        for block_events in &bwe {
            let block_markets = self.extract_markets(block_events);
            new_markets.extend(block_markets);
        }

        tracing::info!("Processed {} blocks with {} new markets", bwe.len(), new_markets.len());

        let mut all_markets = self.lending_repository.get_all_markets().await?;
        all_markets.extend(new_markets.clone());
        let market_map: HashMap<String, Market> =
            all_markets.into_iter().map(|market| (market.id.clone(), market)).collect();

        for block_events in &bwe {
            let block_events = self.extract_lending_events(block_events, &market_map);
            events.extend(block_events);
        }

        Ok(ProcessorOutput::Custom(Arc::new(LendingProcessorOutput {
            markets: new_markets,
            events,
        })))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output {
            if let Some(lending_output) = custom.as_any().downcast_ref::<LendingProcessorOutput>() {
                if !lending_output.markets.is_empty() {
                    self.lending_repository.insert_markets(&lending_output.markets).await?;
                    tracing::info!("Inserted {} new markets", lending_output.markets.len());
                }
                if !lending_output.events.is_empty() {
                    self.lending_repository.insert_lending_events(&lending_output.events).await?;
                    tracing::info!("Inserted {} new events", lending_output.events.len());
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

    fn extract_lending_events(
        &self,
        block_and_events: &BlockAndEvents,
        markets_map: &HashMap<String, Market>,
    ) -> Vec<NewLendingEvent> {
        block_and_events
            .events
            .iter()
            .filter_map(|event| {
                self.parse_lending_event(&block_and_events.block, event, markets_map)
            })
            .collect()
    }

    fn parse_lending_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        if event.contract_address == self.linx_address && event.event_index == 6 {
            self.parse_supply_event(block, event, markets_map)
        } else if event.contract_address == self.linx_address && event.event_index == 7 {
            self.parse_withdraw_event(block, event, markets_map)
        } else if event.contract_address == self.linx_address && event.event_index == 8 {
            self.parse_borrow_event(block, event, markets_map)
        } else if event.contract_address == self.linx_address && event.event_index == 9 {
            self.parse_repay_event(block, event, markets_map)
        } else if event.contract_address == self.linx_address && event.event_index == 10 {
            self.parse_supply_collateral_event(block, event, markets_map)
        } else if event.contract_address == self.linx_address && event.event_index == 11 {
            self.parse_withdraw_collateral_event(block, event, markets_map)
        } else if event.contract_address == self.linx_address && event.event_index == 12 {
            self.parse_liquidate_event(block, event, markets_map)
        } else {
            None
        }
    }

    fn parse_borrow_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m| m.loan_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "Borrow".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 4)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
    }

    fn parse_repay_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m| m.loan_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "Repay".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 3)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
    }

    fn parse_supply_collateral_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m: Market| m.collateral_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "SupplyCollateral".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 3)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
    }

    fn parse_withdraw_collateral_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m: Market| m.collateral_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "WithdrawCollateral".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 4)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
    }

    fn parse_supply_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m| m.loan_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "Supply".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 3)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
    }

    fn parse_withdraw_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m| m.loan_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "Withdraw".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 4)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
    }

    fn parse_liquidate_event(
        &self,
        block: &RichBlockEntry,
        event: &ContractEventByBlockHash,
        markets_map: &HashMap<String, Market>,
    ) -> Option<NewLendingEvent> {
        let market_id = self.extract_string_field(&event.fields, 0)?;
        let market = markets_map.get(&market_id).cloned();
        let token_id = market.map(|m| m.collateral_token.clone()).unwrap_or_default();
        Some(NewLendingEvent {
            market_id,
            event_type: "Liquidate".to_string(),
            token_id,
            on_behalf: self.extract_string_field(&event.fields, 2)?,
            amount: self.extract_bigdecimal_field(&event.fields, 5)?,
            transaction_id: event.tx_id.clone(),
            event_index: event.event_index,
            block_time: chrono::DateTime::from_timestamp(block.timestamp as i64 / 1000, 0)
                .unwrap_or_default()
                .naive_utc(),
            created_at: chrono::Utc::now().naive_utc(),
            fields: serde_json::to_value(&event.fields).unwrap_or_default(),
        })
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
