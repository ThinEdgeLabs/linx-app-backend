use std::sync::Arc;

use anyhow::Result;
use bento_types::DbPool;
use diesel::{
    ExpressionMethods, OptionalExtension,
    query_dsl::methods::{FilterDsl, LimitDsl, OffsetDsl, OrderDsl},
};

use crate::{
    models::{LendingEvent, Market, NewLendingEvent},
    schema,
};
use diesel_async::RunQueryDsl;

pub struct LendingRepository {
    db_pool: Arc<DbPool>,
}

impl LendingRepository {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    pub async fn get_markets(&self, page: i64, limit: i64) -> Result<Vec<Market>> {
        let mut conn = self.db_pool.get().await?;

        let markets: Vec<Market> = schema::lending_markets::table
            .order(schema::lending_markets::created_at.asc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;
        Ok(markets)
    }

    pub async fn get_all_markets(&self) -> Result<Vec<Market>> {
        let mut conn = self.db_pool.get().await?;

        let markets: Vec<Market> = schema::lending_markets::table
            .order(schema::lending_markets::created_at.asc())
            .load(&mut conn)
            .await?;
        Ok(markets)
    }

    pub async fn insert_markets(&self, markets: &[Market]) -> Result<()> {
        if markets.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::lending_markets::table)
            .values(markets)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn get_market(&self, market_id: &str) -> Result<Option<Market>> {
        let mut conn = self.db_pool.get().await?;

        let market: Option<Market> = schema::lending_markets::table
            .filter(schema::lending_markets::id.eq(market_id))
            .first(&mut conn)
            .await
            .optional()?;
        Ok(market)
    }

    pub async fn get_activity(
        &self,
        market_id: String,
        event_types: &[String],
        page: i64,
        limit: i64,
    ) -> Result<Vec<LendingEvent>> {
        let mut conn = self.db_pool.get().await?;

        let events: Vec<LendingEvent> = schema::lending_events::table
            .filter(schema::lending_events::market_id.eq(market_id))
            .filter(schema::lending_events::event_type.eq_any(event_types))
            .order(schema::lending_events::block_time.desc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;
        Ok(events)
    }

    pub async fn insert_lending_events(&self, events: &[NewLendingEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::lending_events::table)
            .values(events)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }
}
