use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::Insertable;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::schema;

#[derive(Insertable, Debug, Clone, PartialEq, Deserialize)]
#[diesel(table_name = schema::account_transactions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewAccountTransaction {
    pub address: String,
    pub tx_type: String,
    pub tx_id: String,
    pub from_group: i16,
    pub to_group: i16,
    pub block_height: i64,
    pub timestamp: NaiveDateTime,
    pub details: JsonValue,
    // Note: tx_key is a generated column, no need to insert it
}

// Helper structs for JSONB details field
// These will be serialized into the details JSONB column

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferDetails {
    pub token_id: String,
    pub from_address: String,
    pub to_address: String,
    pub amount: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwapDetails {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: BigDecimal,
    pub amount_out: BigDecimal,
    pub pool_address: String,
    pub tx_id: String,
    pub hop_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractCallDetails {
    pub contract_address: String,
}

/// Model representing a swap transaction with the information needed for points calculation
#[derive(Debug, Clone)]
pub struct SwapTransaction {
    pub address: String,
    pub tx_id: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: BigDecimal,
    pub amount_out: BigDecimal,
    pub pool_address: String,
    pub hop_count: i32,
}
