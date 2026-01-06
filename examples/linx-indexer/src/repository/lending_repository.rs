use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bento_types::DbPool;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
#[cfg(test)]
use mockall::automock;

use crate::{
    models::{
        LendingEvent, Market, NewLendingEvent, NewMarketStateSnapshot, NewPositionSnapshot,
        Position, PositionSnapshot,
    },
    schema::{self},
};
use diesel_async::RunQueryDsl;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait LendingRepositoryTrait {
    async fn get_borrow_events_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<LendingEvent>>;
    async fn get_position_snapshots_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<PositionSnapshot>>;
}

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
        address: Option<String>,
        page: i64,
        limit: i64,
    ) -> Result<Vec<LendingEvent>> {
        let mut conn = self.db_pool.get().await?;

        let mut query = schema::lending_events::table
            .filter(schema::lending_events::market_id.eq(market_id))
            .filter(schema::lending_events::event_type.eq_any(event_types))
            .into_boxed();

        if let Some(addr) = address {
            query = query.filter(schema::lending_events::on_behalf.eq(addr));
        }

        let events: Vec<LendingEvent> = query
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
                self.calculate_positions_for_market(market_id, page, limit).await
            }
            (None, None) => Err(anyhow::anyhow!("Either market_id or address must be provided")),
        }
    }

    pub async fn insert_position_snapshots(&self, snapshots: &[NewPositionSnapshot]) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::lending_position_snapshots::table)
            .values(snapshots)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn insert_market_state_snapshots(
        &self,
        snapshots: &[NewMarketStateSnapshot],
    ) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get().await?;

        diesel::insert_into(schema::market_state_snapshots::table)
            .values(snapshots)
            .on_conflict((
                schema::market_state_snapshots::market_id,
                schema::market_state_snapshots::snapshot_timestamp,
            ))
            .do_nothing()
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
                    if event.amount > supplied_amount {
                        supplied_amount = BigDecimal::from(0);
                    } else {
                        supplied_amount -= &event.amount;
                    }
                }
                "Borrow" => {
                    borrow_shares += &event.shares;
                    borrowed_amount += &event.amount;
                }
                "Repay" => {
                    borrow_shares -= &event.shares;
                    if event.amount > borrowed_amount {
                        borrowed_amount = BigDecimal::from(0);
                    } else {
                        borrowed_amount -= &event.amount;
                    }
                }
                "SupplyCollateral" => collateral += &event.amount,
                "WithdrawCollateral" => collateral -= &event.amount,
                "Liquidate" => {
                    borrow_shares -= &event.shares;
                    collateral -= &event.amount;

                    let repaid_assets_index = 3;
                    let repaid_assets: Option<bigdecimal::BigDecimal> = event
                        .fields
                        .get(repaid_assets_index)
                        .and_then(|value| value.as_object())
                        .and_then(|obj| obj.get("value"))
                        .and_then(|s| s.as_str())
                        .and_then(|s| s.parse().ok());

                    if let Some(repaid_assets) = repaid_assets {
                        // Repaid assets can be greater than borrowed amount due to interest
                        // accrued. In that case, we set borrowed amount to zero.
                        if repaid_assets > borrowed_amount {
                            borrowed_amount = BigDecimal::from(0);
                        } else {
                            borrowed_amount -= &repaid_assets;
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

    pub async fn get_position_snapshots_in_period(
        &self,
        start_time: chrono::NaiveDateTime,
        end_time: chrono::NaiveDateTime,
    ) -> Result<Vec<crate::models::PositionSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshots: Vec<crate::models::PositionSnapshot> =
            schema::lending_position_snapshots::table
                .filter(schema::lending_position_snapshots::timestamp.ge(start_time))
                .filter(schema::lending_position_snapshots::timestamp.lt(end_time))
                .load(&mut conn)
                .await?;

        Ok(snapshots)
    }

    pub async fn get_borrow_events_in_period(
        &self,
        start_time: chrono::NaiveDateTime,
        end_time: chrono::NaiveDateTime,
    ) -> Result<Vec<LendingEvent>> {
        let mut conn = self.db_pool.get().await?;

        let events: Vec<LendingEvent> = schema::lending_events::table
            .filter(schema::lending_events::event_type.eq("Borrow"))
            .filter(schema::lending_events::block_time.ge(start_time))
            .filter(schema::lending_events::block_time.lt(end_time))
            .load(&mut conn)
            .await?;

        Ok(events)
    }
}

#[async_trait]
impl LendingRepositoryTrait for LendingRepository {
    async fn get_borrow_events_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<LendingEvent>> {
        self.get_borrow_events_in_period(start_time, end_time).await
    }

    async fn get_position_snapshots_in_period(
        &self,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<PositionSnapshot>> {
        self.get_position_snapshots_in_period(start_time, end_time).await
    }
}
