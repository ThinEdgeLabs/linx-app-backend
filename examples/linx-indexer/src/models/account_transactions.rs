use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::{
    Selectable,
    prelude::{AsChangeset, Queryable},
};
use serde::{Deserialize, Serialize};
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
    pub from_group: i16,
    pub to_group: i16,
    pub block_height: i64,
    pub tx_id: String,
    #[schema(value_type = String)]
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "tx_type")]
pub enum AccountTransactionDetails {
    #[serde(rename = "transfer")]
    Transfer(TransferTransactionDto),
    #[serde(rename = "swap")]
    Swap(SwapTransactionDto),
    #[serde(rename = "contract_call")]
    ContractCall(ContractCallTransactionDto),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TransferTransactionDto {
    #[serde(flatten)]
    pub account_transaction: AccountTransaction,
    #[serde(flatten)]
    pub transfer: TransferDetails,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SwapTransactionDto {
    #[serde(flatten)]
    pub account_transaction: AccountTransaction,
    #[serde(flatten)]
    pub swap: SwapDetails,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContractCallTransactionDto {
    #[serde(flatten)]
    pub account_transaction: AccountTransaction,
    #[serde(flatten)]
    pub contract_call: ContractCallDetails,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TransferDetails {
    pub id: i64,
    pub token_id: String,
    pub from_address: String,
    pub to_address: String,
    #[schema(value_type = String)]
    pub amount: BigDecimal,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SwapDetails {
    pub id: i64,
    pub token_in: String,
    pub token_out: String,
    #[schema(value_type = String)]
    pub amount_in: BigDecimal,
    #[schema(value_type = String)]
    pub amount_out: BigDecimal,
    pub pool_address: String,
    pub tx_id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContractCallDetails {
    pub id: i64,
    pub contract_id: String,
}
