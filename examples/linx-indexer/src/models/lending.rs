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
    pub transaction_id: String,
    pub event_index: i32,
    pub block_time: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub fields: Value,
}
