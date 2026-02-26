use crate::BlockAndEvents;

pub mod block;
pub mod event;
pub mod processor_status;
pub mod transaction;

pub use block::BlockModel;
pub use event::EventModel;
pub use transaction::TransactionModel;

pub fn convert_bwe_to_block_models(blocks: Vec<BlockAndEvents>) -> Vec<BlockModel> {
    let mut models = Vec::new();
    for be in blocks {
        let b = be.block;
        models.push(BlockModel {
            hash: b.hash,
            timestamp: crate::utils::timestamp_millis_to_naive_datetime(b.timestamp),
            chain_from: b.chain_from,
            chain_to: b.chain_to,
            height: b.height,
            deps: b.deps.iter().map(|x| Some(x.clone())).collect(),
            nonce: b.nonce,
            version: b.version.to_string(),
            dep_state_hash: b.dep_state_hash,
            txs_hash: b.txs_hash.to_string(),
            tx_number: b.transactions.len() as i64,
            target: b.target,
            main_chain: b.main_chain.unwrap_or(false),
            ghost_uncles: serde_json::to_value(b.ghost_uncles).unwrap_or_default(),
        });
    }
    models
}

pub fn convert_bwe_to_event_models(blocks: Vec<BlockAndEvents>) -> Vec<EventModel> {
    let mut models = Vec::new();

    for be in blocks {
        for e in be.events {
            models.push(EventModel {
                id: uuid::Uuid::new_v4().to_string(),
                tx_id: e.tx_id,
                contract_address: e.contract_address,
                event_index: e.event_index,
                fields: serde_json::to_value(e.fields).unwrap_or_default(), // TODO: need error handling here for retry?
            });
        }
    }
    models
}

pub fn convert_bwe_to_tx_models(blocks: Vec<BlockAndEvents>) -> Vec<TransactionModel> {
    let mut iterator =
        blocks.iter().map(|bwe| (bwe.block.hash.clone(), bwe.block.transactions.clone())).collect::<Vec<_>>();

    iterator
        .iter_mut()
        .flat_map(|(block, transactions)| {
            transactions.dedup_by(|a, b| a.unsigned.tx_id == b.unsigned.tx_id);
            transactions.iter().map(move |t| TransactionModel {
                tx_hash: t.unsigned.tx_id.clone(),
                unsigned: serde_json::to_value(t.unsigned.clone()).unwrap_or_default(),
                script_execution_ok: t.script_execution_ok,
                contract_inputs: serde_json::to_value(t.contract_inputs.clone()).unwrap_or_default(),
                generated_outputs: serde_json::to_value(t.generated_outputs.clone()).unwrap_or_default(),
                input_signatures: t.input_signatures.iter().map(|i| Option::Some(i.to_owned())).collect::<Vec<_>>(),
                script_signatures: t.script_signatures.iter().map(|i| Option::Some(i.to_owned())).collect::<Vec<_>>(),
                block_hash: Some(block.to_string()),
            })
        })
        .collect::<Vec<_>>()
}
