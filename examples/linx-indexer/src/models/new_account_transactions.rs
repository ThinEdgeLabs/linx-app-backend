use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::Insertable;
use serde::Deserialize;

use crate::schema;

#[derive(Insertable, Debug, Clone, PartialEq, Deserialize)]
#[diesel(table_name = schema::account_transactions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewAccountTransaction {
    pub address: String,
    pub tx_type: String,
    pub from_group: i16,
    pub to_group: i16,
    pub block_height: i64,
    pub tx_id: String,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "tx_type")]
pub enum NewAccountTransactionDetails {
    #[serde(rename = "transfer")]
    Transfer(NewTransferTransactionDto),
    #[serde(rename = "swap")]
    Swap(NewSwapTransactionDto),
    #[serde(rename = "contract_call")]
    ContractCall(NewContractCallTransactionDto),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NewTransferTransactionDto {
    #[serde(flatten)]
    pub account_transaction: NewAccountTransaction,
    #[serde(flatten)]
    pub transfer: NewTransferDetails,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NewSwapTransactionDto {
    #[serde(flatten)]
    pub account_transaction: NewAccountTransaction,
    #[serde(flatten)]
    pub swap: NewSwapDetails,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NewContractCallTransactionDto {
    #[serde(flatten)]
    pub account_transaction: NewAccountTransaction,
    #[serde(flatten)]
    pub contract_call: NewContractCallDetails,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NewTransferDetails {
    pub token_id: String,
    pub from_address: String,
    pub to_address: String,
    pub amount: BigDecimal,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NewSwapDetails {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: BigDecimal,
    pub amount_out: BigDecimal,
    pub pool_address: String,
    pub tx_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NewContractCallDetails {
    pub contract_address: String,
}
