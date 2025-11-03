use std::sync::Arc;

use anyhow::Result;
use bento_types::DbPool;
use bigdecimal::BigDecimal;
use diesel::{
    ExpressionMethods, OptionalExtension,
    query_dsl::methods::{FilterDsl, LimitDsl, OffsetDsl, OrderDsl},
};

use crate::{
    models::{LendingEvent, Market, NewDepositSnapshot, NewLendingEvent, Position},
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

    pub async fn get_user_events_for_market(
        &self,
        address: &str,
        market_id: &str,
    ) -> Result<Vec<LendingEvent>> {
        let mut conn = self.db_pool.get().await?;

        let events: Vec<LendingEvent> = schema::lending_events::table
            .filter(schema::lending_events::on_behalf.eq(address))
            .filter(schema::lending_events::market_id.eq(market_id))
            .order(schema::lending_events::block_time.asc())
            .load(&mut conn)
            .await?;

        Ok(events)
    }

    pub async fn get_positions(
        &self,
        market_id: Option<String>,
        address: Option<String>,
        page: i64,
        limit: i64,
    ) -> Result<Vec<Position>> {
        match (market_id.as_ref(), address.as_ref()) {
            (Some(market_id), Some(address)) => {
                if let Some(position) = self.calculate_user_position(address, market_id).await? {
                    Ok(vec![position])
                } else {
                    Ok(vec![])
                }
            }
            (None, Some(address)) => self.calculate_user_positions(address).await,
            (Some(market_id), None) => {
                self.calculate_positions_for_market(&market_id, page, limit).await
            }
            (None, None) => Err(anyhow::anyhow!("Either market_id or address must be provided")),
        }
    }

    pub async fn insert_deposit_snapshots(&self, snapshots: &[NewDepositSnapshot]) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::lending_deposits_snapshots::table)
            .values(snapshots)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    /// Private helper methods

    async fn calculate_user_position(
        &self,
        address: &str,
        market_id: &str,
    ) -> Result<Option<Position>> {
        let events = self.get_user_events_for_market(address, market_id).await?;

        if events.is_empty() {
            return Ok(None);
        }

        let mut supply_shares = BigDecimal::from(0);
        let mut borrow_shares = BigDecimal::from(0);
        let mut collateral = BigDecimal::from(0);
        let mut supplied_amount = BigDecimal::from(0);
        let mut borrowed_amount = BigDecimal::from(0);
        let mut last_updated = events[0].block_time;

        for event in &events {
            match event.event_type.as_str() {
                "Supply" => {
                    supply_shares += &event.shares;
                    supplied_amount += &event.amount;
                }
                "Withdraw" => {
                    supply_shares -= &event.shares;
                    supplied_amount -= &event.amount;
                }
                "Borrow" => {
                    borrow_shares += &event.shares;
                    borrowed_amount += &event.amount;
                }
                "Repay" => {
                    borrow_shares -= &event.shares;
                    borrowed_amount -= &event.amount;
                }
                "SupplyCollateral" => collateral += &event.amount,
                "WithdrawCollateral" => collateral -= &event.amount,
                "Liquidate" => {
                    borrow_shares -= &event.shares;
                    collateral -= &event.amount;
                    // Extract repaidAssets from fields
                    if let Some(repaid_assets) = event.fields.get("repaidAssets") {
                        if let Some(repaid_str) = repaid_assets.as_str() {
                            if let Ok(repaid) = repaid_str.parse::<BigDecimal>() {
                                borrowed_amount -= repaid;
                            }
                        }
                    }
                }
                _ => {}
            }

            if event.block_time > last_updated {
                last_updated = event.block_time;
            }
        }

        if supply_shares == BigDecimal::from(0)
            && borrow_shares == BigDecimal::from(0)
            && collateral == BigDecimal::from(0)
        {
            return Ok(None);
        }

        Ok(Some(Position {
            address: address.to_string(),
            market_id: market_id.to_string(),
            supply_shares,
            borrow_shares,
            collateral,
            supplied_amount,
            borrowed_amount,
            updated_at: last_updated,
        }))
    }

    async fn calculate_positions_for_market(
        &self,
        market_id: &str,
        page: i64,
        limit: i64,
    ) -> Result<Vec<Position>> {
        use diesel::query_dsl::methods::{DistinctDsl, SelectDsl};

        let mut conn = self.db_pool.get().await?;

        let addresses: Vec<String> = schema::lending_events::table
            .filter(schema::lending_events::market_id.eq(market_id))
            .select(schema::lending_events::on_behalf)
            .distinct()
            .order(schema::lending_events::on_behalf.asc())
            .offset((page - 1) * limit)
            .limit(limit)
            .load(&mut conn)
            .await?;

        let mut positions = Vec::new();
        for address in addresses {
            if let Some(position) = self.calculate_user_position(&address, market_id).await? {
                positions.push(position);
            }
        }

        Ok(positions)
    }

    async fn calculate_user_positions(&self, address: &str) -> Result<Vec<Position>> {
        let markets = self.get_all_markets().await?;

        let mut positions = Vec::new();

        for market in markets {
            if let Some(position) = self.calculate_user_position(address, &market.id).await? {
                positions.push(position);
            }
        }

        positions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(positions)
    }
}
