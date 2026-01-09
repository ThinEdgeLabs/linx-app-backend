pub mod config;
pub mod errors;
pub mod models;
pub mod network;
pub mod processors;
pub mod repository;
pub mod schema;
pub mod utils;

use std::fmt::Debug;

use crate::processors::ProcessorOutput;
use diesel_async::{
    pooled_connection::bb8::{Pool, PooledConnection},
    AsyncPgConnection,
};
pub use models::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub type DbPool = Pool<AsyncPgConnection>;
pub type DbPoolConnection<'a> = PooledConnection<'a, AsyncPgConnection>;

pub const DEFAULT_GROUP_NUM: i64 = 4;
pub const REORG_TIMEOUT: i64 = 210 * 16 * 1000; // 210 blocks * 16 seconds
pub const MAX_TIMESTAMP_RANGE: u64 = 1800000;

pub type Event = ContractEventByBlockHash;
pub type BlockHash = String;
pub type GroupIndex = i64;

pub trait CustomProcessorOutput: Send + Sync + Debug + 'static {
    fn as_any(&self) -> &dyn std::any::Any;
    fn clone_box(&self) -> Box<dyn CustomProcessorOutput>;
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeaderEntry {
    pub hash: String,
    pub timestamp: i64,
    pub chain_from: i64,
    pub chain_to: i64,
    pub height: i64,
    pub deps: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockEntry {
    pub hash: String,
    pub timestamp: i64,
    pub chain_from: i64,
    pub chain_to: i64,
    pub height: i64,
    pub deps: Vec<String>,
    pub nonce: String,
    pub version: i8,
    pub dep_state_hash: String,
    pub txs_hash: String,
    pub target: String,
    pub parent: Option<BlockHash>,
    pub main_chain: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RichBlockEntry {
    pub hash: String,
    pub timestamp: i64,
    pub chain_from: i64,
    pub chain_to: i64,
    pub height: i64,
    pub deps: Vec<String>,
    pub transactions: Vec<Transaction>,
    pub nonce: String,
    pub version: i8,
    pub dep_state_hash: String,
    pub txs_hash: String,
    pub target: String,
    pub ghost_uncles: Vec<GhostUncleBlockEntry>,

    pub parent: Option<BlockHash>,
    pub main_chain: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LatestBlock {
    pub hash: String,
    pub timestamp: i64,
    pub chain_from: i64,
    pub chain_to: i64,
    pub height: i64,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostUncleBlockEntry {
    pub block_hash: String,
    pub miner: String,
}

/// Represents the collection of blocks grouped by timestamp ranges.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlocksPerTimestampRange {
    pub blocks: Vec<Vec<RichBlockEntry>>, // A list of block entries per timestamp range.
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, ToSchema)]
pub enum EventFieldType {
    Bool,
    I256,
    U256,
    ByteVec,
    Address,
}

// Parsing event fields helper
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct EventField {
    #[serde(rename = "type")]
    pub field_type: EventFieldType,
    pub value: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockAndEvents {
    pub block: RichBlockEntry,                 // The block entry.
    pub events: Vec<ContractEventByBlockHash>, // The list of events associated with the block.
}

/// Represents a collection of blocks and their associated events, grouped by timestamp range.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlocksAndEventsPerTimestampRange {
    pub blocks_and_events: Vec<Vec<BlockAndEvents>>, // A list of blocks and events grouped by timestamp range.
}

#[derive(Deserialize, Debug, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractEventByBlockHash {
    pub tx_id: String,
    pub contract_address: String,
    pub event_index: i32,
    pub fields: Vec<EventField>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub unsigned: UnsignedTx,                // The unsigned transaction.
    pub script_execution_ok: bool,           // Whether the script execution was successful.
    pub contract_inputs: Vec<ContractInput>, // The contract inputs associated with the transaction.
    pub generated_outputs: Vec<Output>,      // The outputs generated from the transaction.
    pub input_signatures: Vec<String>,       // The signatures of the inputs.
    pub script_signatures: Vec<String>,      // The script signatures for the transaction.
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsignedTx {
    pub tx_id: String,                        // The transaction ID.
    pub version: i32,                         // The version of the transaction.
    pub network_id: i32,                      // The network ID the transaction belongs to.
    pub script_opt: Option<String>,           // Optional script for the transaction.
    pub gas_amount: i32,                      // The gas amount used for the transaction.
    pub gas_price: String,                    // The price of gas for the transaction.
    pub inputs: Vec<RichAssetInput>,          // The inputs of the transaction.
    pub fixed_outputs: Vec<FixedAssetOutput>, // The fixed outputs of the transaction.
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputRef {
    pub hint: i32,   // The hint associated with the output reference.
    pub key: String, // The key for the output reference.
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum OutputType {
    AssetOutput,
    ContractOutput,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    #[serde(rename = "type")]
    pub output_type: OutputType,
    pub hint: i32,
    pub key: String,
    pub atto_alph_amount: String,
    pub address: String,
    pub tokens: Vec<Token>,
}

/// Represents a contract output in a transaction.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ContractOutput {
    pub hint: i32,                // The hint for the contract output.
    pub key: String,              // The key for the contract output.
    pub atto_alph_amount: String, // The amount of atto alph associated with the output.
    pub address: String,          // The address associated with the output.
    pub tokens: Vec<Token>,       // The list of tokens associated with the output.
    pub typ: String,              // The type of the contract output.
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInput {
    pub output_ref: OutputRef, // The output reference for the asset input.
    pub unlock_script: String, // The unlock script for the asset input.
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichAssetInput {
    pub hint: i32,
    pub key: String,
    pub unlock_script: String,
    pub atto_alph_amount: String,
    pub address: String,
    pub tokens: Vec<Token>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractInput {
    pub hint: i32,
    pub key: String,
    pub atto_alph_amount: String,
    pub address: String,
    pub tokens: Vec<Token>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub id: String,
    pub amount: String,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedAssetOutput {
    pub hint: i32,                // The hint for the fixed asset output.
    pub key: String,              // The key for the fixed asset output.
    pub atto_alph_amount: String, // The amount of atto alph associated with the output.
    pub address: String,          // The address associated with the fixed asset output.
    pub tokens: Vec<Token>,       // The tokens associated with the output.
    pub lock_time: i64,           // The lock time for the fixed asset output.
    pub message: String,          // The message for the fixed asset output.
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct TimestampRange {
    pub from: u64,
    pub to: u64,
}

#[derive(Deserialize, Debug, Clone, Serialize, ToSchema)]
pub enum Order {
    Asc,
    Desc,
}

pub enum StageMessage {
    // Input of fetcher stage
    Range(BlockRange),

    // Input of processor stage
    Batch(BlockBatch),

    // Output of processor stage
    Processed(ProcessorOutput),
    Complete,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockRange {
    pub from_ts: u64,
    pub to_ts: u64,
}

#[derive(Clone, Debug)]
pub struct BlockBatch {
    pub blocks: Vec<BlockAndEvents>,
    pub range: BlockRange,
}

#[derive(Deserialize)]
pub struct BlockHashesResponse {
    pub headers: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChainInfo {
    pub current_height: i64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CallContractParams {
    pub group: u32,
    pub world_state_block_hash: Option<String>,
    pub tx_id: Option<String>,
    pub address: String,
    pub method_index: u32,
    pub args: Option<Vec<serde_json::Value>>,
    pub interested_contracts: Option<Vec<String>>,
    pub input_assets: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize, Debug)]
pub enum CallContractResultType {
    CallContractSucceeded,
    CallContractFailed,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CallContractResult {
    #[serde(rename = "type")]
    pub result_type: CallContractResultType,
    pub error: Option<String>,
    pub returns: Option<Vec<serde_json::Value>>,
    pub gas_used: Option<u32>,
    pub contracts: Option<Vec<serde_json::Value>>,
    pub tx_inputs: Option<Vec<serde_json::Value>>,
    pub tx_outputs: Option<Vec<serde_json::Value>>,
    pub events: Option<Vec<serde_json::Value>>,
    pub debug_messages: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Tests the display functionality for Hash and BlockHash types.
    #[test]
    fn test_hash_display() {
        let hash = "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850".to_string();
        assert_eq!(
            format!("{}", hash),
            "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850"
        );

        let block_hash =
            "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850".to_string();
        assert_eq!(
            format!("{}", block_hash),
            "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850"
        );
    }
    #[test]
    fn test_block_entry_deser() {
        let json_data = json!({
            "hash": "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850",
            "parent": "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850",
            "mainChain": true,
            "timestamp": 1672531200,
            "chainFrom": 1,
            "chainTo": 2,
            "height": 1000,
            "deps": ["hash1", "hash2"],
            "transactions": [],
            "nonce": "nonce_value",
            "version": 1,
            "depStateHash": "dep_hash",
            "txsHash": "txs_hash",
            "target": "target_value",
            "ghostUncles": [
                {
                    "blockHash": "unclehash1",
                    "miner": "miner1"
                }
            ]
        });

        let block: RichBlockEntry = serde_json::from_value(json_data).unwrap();

        assert_eq!(block.hash, "00000000000006f8c2bcaac93c5a23df8fba7119ba139d80a49d0303bbf84850");
        assert_eq!(block.timestamp, 1672531200);
        assert_eq!(block.chain_from, 1);
        assert_eq!(block.chain_to, 2);
        assert_eq!(block.height, 1000);
        assert_eq!(block.deps.len(), 2);
        assert_eq!(block.deps[0], "hash1");
        assert_eq!(block.nonce, "nonce_value");
        assert_eq!(block.version, 1);
        assert_eq!(block.dep_state_hash, "dep_hash");
        assert_eq!(block.txs_hash, "txs_hash");
        assert_eq!(block.target, "target_value");
        assert_eq!(block.ghost_uncles.len(), 1);
        assert_eq!(block.ghost_uncles[0].block_hash, "unclehash1");
        assert_eq!(block.ghost_uncles[0].miner, "miner1");
    }

    #[test]
    fn test_transaction_deser() {
        let json_data = json!({
            "unsigned": {
                "txId": "tx123",
                "version": 1,
                "networkId": 42,
                "scriptOpt": "script",
                "gasAmount": 1000,
                "gasPrice": "1000000000",
                "inputs": [],
                "fixedOutputs": []
            },
            "scriptExecutionOk": true,
            "contractInputs": [],
            "generatedOutputs": [],
            "inputSignatures": [],
            "scriptSignatures": []
        });

        let transaction: Transaction = serde_json::from_value(json_data).unwrap();

        assert_eq!(transaction.unsigned.tx_id, "tx123");
        assert!(transaction.script_execution_ok);
        assert_eq!(transaction.unsigned.version, 1);
    }

    #[test]
    fn test_blocks_and_events_deser() {
        let json_data = json!({
            "blocksAndEvents": [
                [
                    {
                        "block": {
                            "hash": "blockhash123",
                            "parent": "parent_hash",
                            "mainChain": true,
                            "timestamp": 1672531200,
                            "chainFrom": 1,
                            "chainTo": 2,
                            "height": 1000,
                            "deps": ["hash1", "hash2"],
                            "transactions": [],
                            "nonce": "nonce_value",
                            "version": 1,
                            "depStateHash": "dep_hash",
                            "txsHash": "txs_hash",
                            "target": "target_value",
                            "ghostUncles": []
                        },
                        "events": []
                    }
                ]
            ]
        });

        let blocks_and_events: BlocksAndEventsPerTimestampRange =
            serde_json::from_value(json_data).unwrap();

        assert_eq!(blocks_and_events.blocks_and_events.len(), 1);
        let block_and_event = &blocks_and_events.blocks_and_events[0][0];
        assert_eq!(block_and_event.block.hash, "blockhash123");
    }

    #[test]
    fn test_event_deser() {
        let json_data = json!(
            {
                "contractAddress": "tgx7VNFoP9DJiFMFgXXtafQZkUvyEdDHT9ryamHJZC9M",
                "txId": "585cda67fae0756b9a43ff30e3738e0ee4b7ed4286c66e2a51b9822f3dfa8899",
                "eventIndex": -1,
                "fields": [
                    {
                        "type": "Address",
                        "value": "25krLqkUUDUYUmqdzZPmXVTHTSQos6UZQ4H6xhRSjB1Yj"
                    },
                    {
                        "type": "Address",
                        "value": "yuF1Sum4ricLFBc86h3RdjFsebR7ZXKBHm2S5sZmVsiF"
                    },
                    {
                        "type": "ByteVec",
                        "value": ""
                    }
                ]
            }
        );
        let event: Event = serde_json::from_value(json_data).unwrap();
        assert_eq!(
            event.contract_address,
            "tgx7VNFoP9DJiFMFgXXtafQZkUvyEdDHT9ryamHJZC9M".to_string()
        );
        assert_eq!(
            event.tx_id,
            "585cda67fae0756b9a43ff30e3738e0ee4b7ed4286c66e2a51b9822f3dfa8899".to_string()
        );
        assert_eq!(event.event_index, -1);
        assert_eq!(event.fields.len(), 3);

        let field = &event.fields[0];
        assert_eq!(field.field_type, EventFieldType::Address);

        let field = &event.fields[1];
        assert_eq!(field.field_type, EventFieldType::Address);

        let field = &event.fields[2];
        assert_eq!(field.field_type, EventFieldType::ByteVec);
    }
}
