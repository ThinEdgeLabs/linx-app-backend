use crate::schema;
use bigdecimal::BigDecimal;

use chrono::{Duration, NaiveDateTime, Utc};

use diesel::prelude::{AsChangeset, Insertable, Queryable};
use diesel::sql_types::{Numeric, Timestamp};
use diesel::QueryableByName;
use serde::{Deserialize, Serialize};
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
#[diesel(table_name = schema::lending_position_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PositionSnapshot {
    pub id: i64,
    pub address: String,
    pub market_id: String,
    #[schema(value_type = String)]
    pub supply_amount: BigDecimal,
    #[schema(value_type = String)]
    pub supply_amount_usd: BigDecimal,
    #[schema(value_type = String)]
    pub timestamp: NaiveDateTime,
    #[schema(value_type = String)]
    pub borrow_amount: BigDecimal,
    #[schema(value_type = String)]
    pub borrow_amount_usd: BigDecimal,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::lending_position_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewPositionSnapshot {
    pub address: String,
    pub market_id: String,
    pub supply_amount: BigDecimal,
    pub supply_amount_usd: BigDecimal,
    pub timestamp: NaiveDateTime,
    pub borrow_amount: BigDecimal,
    pub borrow_amount_usd: BigDecimal,
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

#[derive(Queryable, Insertable, Debug, Clone, Serialize, AsChangeset, ToSchema)]
#[diesel(table_name = schema::market_state_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct MarketStateSnapshot {
    pub id: i32,
    pub market_id: String,
    #[schema(value_type = String)]
    pub total_supply_assets: BigDecimal,
    #[schema(value_type = String)]
    pub total_supply_shares: BigDecimal,
    #[schema(value_type = String)]
    pub total_borrow_assets: BigDecimal,
    #[schema(value_type = String)]
    pub total_borrow_shares: BigDecimal,
    #[schema(value_type = String)]
    pub interest_rate: Option<BigDecimal>,
    #[schema(value_type = String)]
    pub snapshot_timestamp: NaiveDateTime,
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = schema::market_state_snapshots)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewMarketStateSnapshot {
    pub market_id: String,
    pub total_supply_assets: BigDecimal,
    pub total_supply_shares: BigDecimal,
    pub total_borrow_assets: BigDecimal,
    pub total_borrow_shares: BigDecimal,
    pub interest_rate: Option<BigDecimal>,
    pub snapshot_timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
pub enum Timeframe {
    #[serde(rename = "1m")]
    OneMonth,
    #[serde(rename = "3m")]
    ThreeMonths,
    #[serde(rename = "1y")]
    OneYear,
    #[serde(rename = "all")]
    All,
}

impl Timeframe {
    pub fn start_time(&self) -> NaiveDateTime {
        let now = Utc::now().naive_utc();
        match self {
            Timeframe::OneMonth => now - Duration::days(30),
            Timeframe::ThreeMonths => now - Duration::days(90),
            Timeframe::OneYear => now - Duration::days(365),
            Timeframe::All => chrono::DateTime::UNIX_EPOCH.naive_utc(),
        }
    }

    pub fn bucket_interval(&self) -> &'static str {
        match self {
            Timeframe::OneMonth => "hour",
            Timeframe::ThreeMonths | Timeframe::OneYear | Timeframe::All => "day",
        }
    }
}

#[derive(QueryableByName, Debug, Clone, Serialize, ToSchema)]
pub struct UserPositionHistoryPoint {
    #[diesel(sql_type = Timestamp)]
    #[schema(value_type = String)]
    pub timestamp: NaiveDateTime,
    #[diesel(sql_type = Numeric)]
    #[schema(value_type = String)]
    pub supply_amount_usd: BigDecimal,
    #[diesel(sql_type = Numeric)]
    #[schema(value_type = String)]
    pub borrow_amount_usd: BigDecimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_start_time_one_month() {
        let now = Utc::now().naive_utc();
        let start = Timeframe::OneMonth.start_time();
        let diff = now - start;
        // Should be approximately 30 days (allow 1 second tolerance)
        assert!(diff.num_days() == 30 || diff.num_days() == 29);
    }

    #[test]
    fn test_start_time_three_months() {
        let now = Utc::now().naive_utc();
        let start = Timeframe::ThreeMonths.start_time();
        let diff = now - start;
        assert!(diff.num_days() == 90 || diff.num_days() == 89);
    }

    #[test]
    fn test_start_time_one_year() {
        let now = Utc::now().naive_utc();
        let start = Timeframe::OneYear.start_time();
        let diff = now - start;
        assert!(diff.num_days() == 365 || diff.num_days() == 364);
    }

    #[test]
    fn test_start_time_all() {
        let start = Timeframe::All.start_time();
        assert_eq!(start, chrono::DateTime::UNIX_EPOCH.naive_utc());
    }

    #[test]
    fn test_bucket_interval_hourly() {
        assert_eq!(Timeframe::OneMonth.bucket_interval(), "hour");
    }

    #[test]
    fn test_bucket_interval_daily() {
        assert_eq!(Timeframe::ThreeMonths.bucket_interval(), "day");
        assert_eq!(Timeframe::OneYear.bucket_interval(), "day");
        assert_eq!(Timeframe::All.bucket_interval(), "day");
    }

    #[test]
    fn test_serde_deserialize_valid() {
        let one_month: Timeframe = serde_json::from_str("\"1m\"").unwrap();
        assert!(matches!(one_month, Timeframe::OneMonth));

        let three_months: Timeframe = serde_json::from_str("\"3m\"").unwrap();
        assert!(matches!(three_months, Timeframe::ThreeMonths));

        let one_year: Timeframe = serde_json::from_str("\"1y\"").unwrap();
        assert!(matches!(one_year, Timeframe::OneYear));

        let all: Timeframe = serde_json::from_str("\"all\"").unwrap();
        assert!(matches!(all, Timeframe::All));
    }

    #[test]
    fn test_serde_deserialize_invalid() {
        assert!(serde_json::from_str::<Timeframe>("\"2m\"").is_err());
        assert!(serde_json::from_str::<Timeframe>("\"\"").is_err());
        assert!(serde_json::from_str::<Timeframe>("\"6m\"").is_err());
        assert!(serde_json::from_str::<Timeframe>("\"weekly\"").is_err());
    }
}
