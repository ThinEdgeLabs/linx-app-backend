use crate::schema;
use bigdecimal::BigDecimal;

use chrono::NaiveDateTime;

use diesel::prelude::{AsChangeset, Insertable, Queryable};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::lending_markets)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Market {
    pub id: String,
    pub market_contract_id: String,
    pub collateral_token: String,
    pub loan_token: String,
    pub oracle: String,
    pub irm: String,
    #[schema(value_type = String)]
    pub ltv: BigDecimal,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::lending_events)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct LendingEvent {
    pub id: i64,
    pub market_id: String,
    pub event_type: String,
    pub token_id: String,
    pub on_behalf: String,
    #[schema(value_type = String)]
    pub amount: BigDecimal,
    #[schema(value_type = String)]
    pub shares: BigDecimal,
    pub transaction_id: String,
    pub event_index: i32,
    #[schema(value_type = String)]
    pub block_time: NaiveDateTime,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
    #[serde(skip)]
    #[schema(ignore)]
    pub fields: Value,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::lending_events)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewLendingEvent {
    pub market_id: String,
    pub event_type: String,
    pub token_id: String,
    pub on_behalf: String,
    pub amount: BigDecimal,
    pub shares: BigDecimal,
    pub transaction_id: String,
    pub event_index: i32,
    pub block_time: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub fields: Value,
}

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::lending_deposits_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct DepositSnapshot {
    pub id: i64,
    pub address: String,
    pub market_id: String,
    #[schema(value_type = String)]
    pub amount: BigDecimal,
    #[schema(value_type = String)]
    pub amount_usd: BigDecimal,
    #[schema(value_type = String)]
    pub timestamp: NaiveDateTime,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::lending_deposits_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewDepositSnapshot {
    pub address: String,
    pub market_id: String,
    pub amount: BigDecimal,
    pub amount_usd: BigDecimal,
    pub timestamp: NaiveDateTime,
}

#[derive(Serialize, ToSchema, Debug)]
pub struct Position {
    pub market_id: String,
    pub address: String,
    #[schema(value_type = String)]
    pub supply_shares: BigDecimal,
    #[schema(value_type = String)]
    pub borrow_shares: BigDecimal,
    #[schema(value_type = String)]
    pub collateral: BigDecimal,
    #[schema(value_type = String)]
    pub supplied_amount: BigDecimal,
    #[schema(value_type = String)]
    pub borrowed_amount: BigDecimal,
    #[schema(value_type = String)]
    pub updated_at: NaiveDateTime,
}

#[derive(Debug)]
pub struct MarketState {
    pub total_supply_assets: BigDecimal,
    pub total_supply_shares: BigDecimal,
    pub total_borrow_assets: BigDecimal,
    pub total_borrow_shares: BigDecimal,
    pub last_update: NaiveDateTime,
    pub fee: BigDecimal,
}
