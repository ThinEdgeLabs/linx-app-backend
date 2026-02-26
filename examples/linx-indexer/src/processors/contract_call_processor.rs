use anyhow::Result;
use std::{collections::HashSet, fmt::Debug, sync::Arc};

use async_trait::async_trait;
use bento_core::{ProcessorFactory, db::DbPool};
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    BlockAndEvents, CustomProcessorOutput, RichBlockEntry, Transaction, processors::ProcessorOutput,
    utils::timestamp_millis_to_naive_datetime,
};

use crate::{
    models::{ContractCallDetails, NewAccountTransaction},
    processors::classifier::{TransactionCategory, TransactionClassifier},
    repository::AccountTransactionRepository,
};
pub struct ContractCallProcessor {
    connection_pool: Arc<DbPool>,
    repository: AccountTransactionRepository,
    classifier: TransactionClassifier,
}

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, config: Option<Arc<dyn bento_types::config::AppConfigTrait>>| {
        Box::new(ContractCallProcessor::new(db_pool, config))
    }
}

impl ContractCallProcessor {
    pub fn new(connection_pool: Arc<DbPool>, _config: Option<Arc<dyn bento_types::config::AppConfigTrait>>) -> Self {
        tracing::debug!("Initialized ContractCallProcessor");
        let repository = AccountTransactionRepository::new(connection_pool.clone());
        let classifier = TransactionClassifier::new(HashSet::new());
        Self { connection_pool, repository, classifier }
    }
}

impl Debug for ContractCallProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "ContractCallProcessor {{ connections: {:?}, idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[derive(Debug, Clone)]
pub struct ContractCallProcessorOutput {
    pub contract_calls: Vec<NewAccountTransaction>,
}

impl CustomProcessorOutput for ContractCallProcessorOutput {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl ProcessorTrait for ContractCallProcessor {
    fn name(&self) -> &'static str {
        "contract_call"
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, bwe: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let contract_calls = bwe
            .iter()
            .flat_map(|el| {
                el.block
                    .transactions
                    .iter()
                    .filter(|tx| self.classifier.classify(tx) == TransactionCategory::ContractCall)
                    .filter_map(|tx| extract_contract_call(tx, &el.block))
            })
            .collect();

        Ok(ProcessorOutput::Custom(Arc::new(ContractCallProcessorOutput { contract_calls })))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output {
            if let Some(contract_call_output) = custom.as_any().downcast_ref::<ContractCallProcessorOutput>() {
                let contract_calls = &contract_call_output.contract_calls;
                if !contract_calls.is_empty() {
                    self.repository.insert_transactions(contract_calls).await?;
                    tracing::info!("Inserted {} contract calls", contract_calls.len());
                }
            } else {
                return Err(anyhow::anyhow!("Invalid custom output type"));
            }
        } else {
            return Err(anyhow::anyhow!("Expected Custom output type"));
        }

        Ok(())
    }
}

pub fn extract_contract_call(tx: &Transaction, block: &RichBlockEntry) -> Option<NewAccountTransaction> {
    // NOTE: Choosing the first input is possibly not the best approach.
    // Also if the transaction has no inputs we cannot identify the sender.
    let address = tx.unsigned.inputs.first().map(|input| &input.address);
    let contract_address = tx.contract_inputs.first().map(|input| &input.address);

    if address.is_none() || contract_address.is_none() {
        return None;
    }

    let contract_call_details = ContractCallDetails { contract_address: contract_address.unwrap().to_string() };

    let details_json = serde_json::to_value(&contract_call_details).ok()?;

    Some(NewAccountTransaction {
        address: address.unwrap().to_string(),
        tx_type: "contract_call".to_string(),
        tx_id: tx.unsigned.tx_id.to_string(),
        from_group: block.chain_from as i16,
        to_group: block.chain_to as i16,
        block_height: block.height,
        timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
        details: details_json,
    })
}
