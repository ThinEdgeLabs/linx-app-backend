use std::{collections::HashMap, fmt::Debug, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use bento_core::{ProcessorFactory, db::DbPool};
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    BlockAndEvents, ContractEventByBlockHash, CustomProcessorOutput, EventField, RichBlockEntry,
    processors::ProcessorOutput, utils::timestamp_millis_to_naive_datetime,
};
use bigdecimal::Zero;
use serde_json::Value;

use crate::{
    address_from_contract_id,
    models::{NewAccountTransaction, NewPoolDto, NewSwapDetails, NewSwapTransactionDto, Pool},
    repository::{AccountTransactionRepository, PoolRepository},
};

const AYIN_V2_FACTORY_ADDRESS: &str = "vyrkJHG49TXss6pGAz2dVxq5o7mBXNNXAV18nAeqVT1R";
const ELEXIUM_FACTORY_ADDRESS: &str = "22oTtDJEMjNc9QAdmcZarnEzgkAooJp9gZy7RYBisniR5";

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, args: Option<Value>| {
        let processor = DexProcessor::new(db_pool, args);
        Box::new(processor)
    }
}

pub struct DexProcessor {
    connection_pool: Arc<DbPool>,
    swap_repository: AccountTransactionRepository,
    pool_repository: PoolRepository,
}

impl DexProcessor {
    pub fn new(connection_pool: Arc<DbPool>, _args: Option<Value>) -> Self {
        let swap_repository = AccountTransactionRepository::new(connection_pool.clone());
        let pool_repository = PoolRepository::new(connection_pool.clone());

        Self { connection_pool, swap_repository, pool_repository }
    }

    fn extract_new_pools(&self, events: &[ContractEventByBlockHash]) -> Vec<NewPoolDto> {
        events.iter().filter_map(|event| self.parse_pool_creation_event(event)).collect()
    }

    /// Parse pool creation event for different DEX factories
    fn parse_pool_creation_event(&self, event: &ContractEventByBlockHash) -> Option<NewPoolDto> {
        match event.contract_address.as_str() {
            AYIN_V2_FACTORY_ADDRESS if event.event_index == 0 => {
                self.parse_ayin_v2_pool_event(event)
            }
            ELEXIUM_FACTORY_ADDRESS if event.event_index == 0 => {
                self.parse_elexium_pool_event(event)
            }
            _ => None,
        }
    }

    /// Parse Ayin V2 pool creation event
    fn parse_ayin_v2_pool_event(&self, event: &ContractEventByBlockHash) -> Option<NewPoolDto> {
        if event.fields.len() < 4 {
            tracing::warn!(
                "Ayin V2 pool creation event has insufficient fields: {}",
                event.fields.len()
            );
            return None;
        }

        let token_a = self.extract_string_field(&event.fields, 0)?;
        let token_b = self.extract_string_field(&event.fields, 1)?;
        let contract_id = self.extract_string_field(&event.fields, 2)?;

        tracing::debug!(
            "Parsed Ayin V2 pool: token_a={}, token_b={}, contract_id={}",
            token_a,
            token_b,
            contract_id
        );

        Some(NewPoolDto {
            address: address_from_contract_id(&contract_id),
            token_a,
            token_b,
            factory_address: AYIN_V2_FACTORY_ADDRESS.to_string(),
        })
    }

    /// Parse Elexium pool creation event
    fn parse_elexium_pool_event(&self, event: &ContractEventByBlockHash) -> Option<NewPoolDto> {
        if event.fields.len() < 5 {
            tracing::warn!(
                "Elexium pool creation event has insufficient fields: {}",
                event.fields.len()
            );
            return None;
        }

        let token_a = self.extract_string_field(&event.fields, 0)?;
        let token_b = self.extract_string_field(&event.fields, 1)?;
        let contract_id = self.extract_string_field(&event.fields, 3)?;

        tracing::debug!(
            "Parsed Elexium pool: token_a={}, token_b={}, contract_id={}",
            token_a,
            token_b,
            contract_id
        );

        Some(NewPoolDto {
            address: address_from_contract_id(&contract_id),
            token_a,
            token_b,
            factory_address: ELEXIUM_FACTORY_ADDRESS.to_string(),
        })
    }

    fn extract_swaps(
        &self,
        bwe: &BlockAndEvents,
        pools: &HashMap<String, Pool>,
    ) -> Vec<NewSwapTransactionDto> {
        bwe.events
            .iter()
            .filter_map(|event| self.parse_swap_event(event, pools, &bwe.block))
            .collect()
    }

    fn parse_swap_event(
        &self,
        event: &ContractEventByBlockHash,
        pools: &HashMap<String, Pool>,
        block: &RichBlockEntry,
    ) -> Option<NewSwapTransactionDto> {
        match pools.get(event.contract_address.as_str()) {
            Some(pool) if pool.factory_address == AYIN_V2_FACTORY_ADDRESS => {
                self.parse_ayin_v2_swap_event(event, pool, block)
            }
            Some(pool) if pool.factory_address == ELEXIUM_FACTORY_ADDRESS => {
                self.parse_elexium_swap_event(event, pool, block)
            }
            _ => None,
        }
    }

    fn parse_elexium_swap_event(
        &self,
        event: &ContractEventByBlockHash,
        pool: &Pool,
        block: &RichBlockEntry,
    ) -> Option<NewSwapTransactionDto> {
        // Elexium swap events have 6 fields and event_index 3
        if event.fields.len() != 6 || event.event_index != 3 {
            return None;
        }

        let sender = self.extract_string_field(&event.fields, 0)?;
        let token_a_in = self.extract_bigdecimal_field(&event.fields, 1)?;
        let token_b_in = self.extract_bigdecimal_field(&event.fields, 2)?;
        let token_a_out = self.extract_bigdecimal_field(&event.fields, 3)?;
        let token_b_out = self.extract_bigdecimal_field(&event.fields, 4)?;

        // Determine swap direction
        let swap = if token_a_in.is_zero() {
            let token_in = pool.token_b.clone();
            let token_out = pool.token_a.clone();
            let amount_in = token_b_in;
            let amount_out = token_a_out;
            NewSwapDetails {
                token_in,
                token_out,
                amount_in,
                amount_out,
                pool_address: event.contract_address.clone(),
                tx_id: event.tx_id.to_string(),
            }
        } else {
            let token_in = pool.token_a.clone();
            let token_out = pool.token_b.clone();
            let amount_in = token_a_in;
            let amount_out = token_b_out;
            NewSwapDetails {
                token_in,
                token_out,
                amount_in,
                amount_out,
                pool_address: event.contract_address.clone(),
                tx_id: event.tx_id.to_string(),
            }
        };

        Some(NewSwapTransactionDto {
            account_transaction: NewAccountTransaction {
                address: sender,
                tx_type: "swap".to_string(),
                from_group: block.chain_from as i16,
                to_group: block.chain_to as i16,
                block_height: block.height,
                tx_id: event.tx_id.to_string(),
                timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
            },
            swap,
        })
    }

    fn parse_ayin_v2_swap_event(
        &self,
        event: &ContractEventByBlockHash,
        pool: &Pool,
        block: &RichBlockEntry,
    ) -> Option<NewSwapTransactionDto> {
        // Ayin V2 swap events have 6 fields and event_index 2
        if event.fields.len() != 6 || event.event_index != 2 {
            return None;
        }

        let sender = self.extract_string_field(&event.fields, 0)?;
        //let receiver = self.extract_string_field(&event.fields, 5)?;

        let token_a_in = self.extract_bigdecimal_field(&event.fields, 1)?;
        let token_b_in = self.extract_bigdecimal_field(&event.fields, 2)?;
        let token_a_out = self.extract_bigdecimal_field(&event.fields, 3)?;
        let token_b_out = self.extract_bigdecimal_field(&event.fields, 4)?;

        // Determine swap direction
        let swap = if token_a_in.is_zero() {
            let token_in = pool.token_b.clone();
            let token_out = pool.token_a.clone();
            let amount_in = token_b_in;
            let amount_out = token_a_out;
            NewSwapDetails {
                token_in,
                token_out,
                amount_in,
                amount_out,
                pool_address: event.contract_address.clone(),
                tx_id: event.tx_id.to_string(),
            }
        } else {
            let token_in = pool.token_a.clone();
            let token_out = pool.token_b.clone();
            let amount_in = token_a_in;
            let amount_out = token_b_out;
            NewSwapDetails {
                token_in,
                token_out,
                amount_in,
                amount_out,
                pool_address: event.contract_address.clone(),
                tx_id: event.tx_id.to_string(),
            }
        };

        Some(NewSwapTransactionDto {
            account_transaction: NewAccountTransaction {
                address: sender,
                tx_type: "swap".to_string(),
                from_group: block.chain_from as i16,
                to_group: block.chain_to as i16,
                block_height: block.height,
                tx_id: event.tx_id.to_string(),
                timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
            },
            swap,
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

#[derive(Debug, Clone)]
pub struct DexProcessorOutput {
    pub new_pools: Vec<NewPoolDto>,
    pub swaps: Vec<NewSwapTransactionDto>,
}

impl CustomProcessorOutput for DexProcessorOutput {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> {
        Box::new(self.clone())
    }
}

impl Debug for DexProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DexProcessor")
    }
}

#[async_trait]
impl ProcessorTrait for DexProcessor {
    fn name(&self) -> &'static str {
        "dex"
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, bwe: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let mut swaps = Vec::new();
        // This might be an issue if the number of pools is large
        let mut existing_pools = self.pool_repository.get_pools().await?;
        let mut new_pools: Vec<NewPoolDto> = Vec::new();

        for block_events in &bwe {
            let block_pools = self.extract_new_pools(&block_events.events);
            existing_pools
                .extend(block_pools.iter().map(|p| (p.address.clone(), Pool::from(p.clone()))));
            new_pools.extend(block_pools);

            let block_swaps = self.extract_swaps(&block_events, &existing_pools);
            swaps.extend(block_swaps);
        }

        tracing::info!(
            "Processed {} blocks with {} new pools and {} swaps",
            bwe.len(),
            new_pools.len(),
            swaps.len()
        );

        Ok(ProcessorOutput::Custom(Arc::new(DexProcessorOutput { new_pools, swaps })))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output {
            if let Some(dex_output) = custom.as_any().downcast_ref::<DexProcessorOutput>() {
                if !dex_output.new_pools.is_empty() {
                    self.pool_repository.insert_pools(&dex_output.new_pools).await?;
                    tracing::info!("Inserted {} new pools", dex_output.new_pools.len());
                }

                if !dex_output.swaps.is_empty() {
                    self.swap_repository.insert_swaps(&dex_output.swaps).await?;
                    tracing::info!("Inserted {} swaps", dex_output.swaps.len());
                }
            }
        }
        Ok(())
    }
}
