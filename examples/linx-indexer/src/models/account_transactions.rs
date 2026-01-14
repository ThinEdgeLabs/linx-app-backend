use chrono::NaiveDateTime;
use diesel::{
    Selectable,
    prelude::{AsChangeset, Queryable},
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use utoipa::ToSchema;

use crate::schema;

#[derive(
    Queryable, Selectable, Debug, Clone, Serialize, AsChangeset, PartialEq, Deserialize, ToSchema,
)]
#[diesel(table_name = schema::account_transactions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct AccountTransaction {
    pub id: i64,
    pub address: String,
    pub tx_type: String,
    pub tx_id: String,
    pub from_group: i16,
    pub to_group: i16,
    pub block_height: i64,
    #[schema(value_type = String)]
    pub timestamp: NaiveDateTime,
    pub details: JsonValue,
    pub tx_key: String,
}

/// Flattened response DTO for account transactions
/// Merges the JSONB details field into the root object
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AccountTransactionFlattened {
    pub id: i64,
    pub address: String,
    pub tx_type: String,
    pub tx_id: String,
    pub from_group: i16,
    pub to_group: i16,
    pub block_height: i64,
    #[schema(value_type = String)]
    pub timestamp: NaiveDateTime,
    #[serde(flatten)]
    pub details: JsonValue,
}

impl From<AccountTransaction> for AccountTransactionFlattened {
    fn from(tx: AccountTransaction) -> Self {
        Self {
            id: tx.id,
            address: tx.address,
            tx_type: tx.tx_type,
            tx_id: tx.tx_id,
            from_group: tx.from_group,
            to_group: tx.to_group,
            block_height: tx.block_height,
            timestamp: tx.timestamp,
            details: tx.details,
        }
    }
}
