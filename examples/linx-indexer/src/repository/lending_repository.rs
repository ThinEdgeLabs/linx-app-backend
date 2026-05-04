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
        Apy30d, LendingEvent, LendingStatsSnapshot, Market, MarketStatePoint, MarketStateSnapshot, NewLendingEvent,
        NewLendingStatsSnapshot, NewMarketStateSnapshot, NewPositionSnapshot, Position, PositionSnapshot,
        PositionTotals, SeriesBucket, Timeframe, UserPositionHistoryPoint,
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

        let markets: Vec<Market> =
            schema::lending_markets::table.order(schema::lending_markets::created_at.asc()).load(&mut conn).await?;
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

    pub async fn get_user_events_for_market(&self, address: &str, market_id: &str) -> Result<Vec<LendingEvent>> {
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
            (Some(market_id), None) => self.calculate_positions_for_market(market_id, page, limit).await,
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

    pub async fn insert_market_state_snapshots(&self, snapshots: &[NewMarketStateSnapshot]) -> Result<()> {
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

    /// `(id, snapshot_timestamp)` for every snapshot of a market, ascending by timestamp.
    pub async fn list_snapshot_ids_for_market(&self, market_id_value: &str) -> Result<Vec<(i32, NaiveDateTime)>> {
        use schema::market_state_snapshots::dsl::*;
        let mut conn = self.db_pool.get().await?;
        let rows: Vec<(i32, NaiveDateTime)> = market_state_snapshots
            .filter(market_id.eq(market_id_value))
            .select((id, snapshot_timestamp))
            .order(snapshot_timestamp.asc())
            .load(&mut conn)
            .await?;
        Ok(rows)
    }

    pub async fn update_cumulative_volumes(
        &self,
        snapshot_id: i32,
        supply_usd: &BigDecimal,
        borrow_usd: &BigDecimal,
    ) -> Result<()> {
        use schema::market_state_snapshots::dsl::*;
        let mut conn = self.db_pool.get().await?;
        diesel::update(market_state_snapshots.filter(id.eq(snapshot_id)))
            .set((cumulative_supply_volume_usd.eq(supply_usd), cumulative_borrow_volume_usd.eq(borrow_usd)))
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    /// Latest snapshot per market (DISTINCT ON (market_id) ORDER BY snapshot_timestamp DESC).
    pub async fn get_latest_market_state_snapshots(&self) -> Result<Vec<MarketStateSnapshot>> {
        use schema::market_state_snapshots::dsl::*;
        let mut conn = self.db_pool.get().await?;
        let rows: Vec<MarketStateSnapshot> = market_state_snapshots
            .distinct_on(market_id)
            .order((market_id, snapshot_timestamp.desc()))
            .load(&mut conn)
            .await?;
        Ok(rows)
    }

    /// Last snapshot per (market, time bucket) within a rolling window.
    /// `window_seconds` defines the lookback (`NOW() - window_seconds`).
    /// `market_id_filter = None` returns rows for every market.
    pub async fn get_market_state_points(
        &self,
        market_id_filter: Option<&str>,
        bucket: SeriesBucket,
        window_seconds: i64,
    ) -> Result<Vec<MarketStatePoint>> {
        let mut conn = self.db_pool.get().await?;
        let mut sql = format!(
            "SELECT DISTINCT ON (market_id, bucket_ts) \
                    market_id, \
                    date_trunc('{}', snapshot_timestamp) AS bucket_ts, \
                    total_supply_usd, total_borrow_usd, total_collateral_usd, \
                    total_supply_assets, total_borrow_assets, \
                    borrow_apy, fee, \
                    cumulative_supply_volume_usd, cumulative_borrow_volume_usd, \
                    bad_debt_usd \
             FROM market_state_snapshots \
             WHERE snapshot_timestamp >= NOW() - make_interval(secs => $1)",
            bucket.date_trunc_kind(),
        );

        let rows = match market_id_filter {
            Some(market_id_value) => {
                sql.push_str(" AND market_id = $2 ORDER BY market_id, bucket_ts, snapshot_timestamp DESC");
                diesel::sql_query(sql)
                    .bind::<diesel::sql_types::BigInt, _>(window_seconds)
                    .bind::<diesel::sql_types::Text, _>(market_id_value)
                    .load::<MarketStatePoint>(&mut conn)
                    .await?
            }
            None => {
                sql.push_str(" ORDER BY market_id, bucket_ts, snapshot_timestamp DESC");
                diesel::sql_query(sql)
                    .bind::<diesel::sql_types::BigInt, _>(window_seconds)
                    .load::<MarketStatePoint>(&mut conn)
                    .await?
            }
        };
        Ok(rows)
    }

    /// Total realized bad debt (raw loan-token assets) for a market, summed across
    /// every Liquidate event. The `badDebtAssets` value lives at field index 6 of the
    /// Liquidate event payload, which the processor stores in the `fields` jsonb column.
    pub async fn sum_bad_debt_assets(&self, market_id_value: &str) -> Result<BigDecimal> {
        #[derive(diesel::QueryableByName)]
        struct Row {
            #[diesel(sql_type = diesel::sql_types::Numeric)]
            total: BigDecimal,
        }
        let mut conn = self.db_pool.get().await?;
        let row: Row = diesel::sql_query(
            "SELECT COALESCE(SUM((fields->6->>'value')::numeric), 0) AS total \
             FROM lending_events \
             WHERE event_type = 'Liquidate' AND market_id = $1",
        )
        .bind::<diesel::sql_types::Text, _>(market_id_value)
        .get_result(&mut conn)
        .await?;
        Ok(row.total)
    }

    /// SUM(amount) of lending events of a given type for a market within a half-open
    /// time window `(since, until]`.
    pub async fn sum_event_amounts(
        &self,
        market_id_value: &str,
        event_type_value: &str,
        since: NaiveDateTime,
        until: NaiveDateTime,
    ) -> Result<BigDecimal> {
        use schema::lending_events::dsl::*;
        let mut conn = self.db_pool.get().await?;
        let total: Option<BigDecimal> = lending_events
            .filter(market_id.eq(market_id_value))
            .filter(event_type.eq(event_type_value))
            .filter(block_time.gt(since))
            .filter(block_time.le(until))
            .select(diesel::dsl::sum(amount))
            .first(&mut conn)
            .await?;
        Ok(total.unwrap_or_default())
    }

    pub async fn get_user_position_history(
        &self,
        address: &str,
        market_id: Option<&str>,
        timeframe: Timeframe,
    ) -> Result<Vec<UserPositionHistoryPoint>> {
        let mut conn = self.db_pool.get().await?;
        let start_time = timeframe.start_time();
        let bucket_interval = timeframe.bucket_interval();

        let results = if let Some(market_id) = market_id {
            // Pick the last snapshot per bucket.
            diesel::sql_query(format!(
                "SELECT bucket_ts as timestamp, supply_amount_usd, borrow_amount_usd \
                 FROM ( \
                     SELECT DISTINCT ON (date_trunc('{0}', timestamp)) \
                            date_trunc('{0}', timestamp) as bucket_ts, \
                            supply_amount_usd, \
                            borrow_amount_usd \
                     FROM lending_position_snapshots \
                     WHERE address = $1 AND market_id = $2 AND timestamp >= $3 \
                     ORDER BY date_trunc('{0}', timestamp), timestamp DESC \
                 ) sub \
                 ORDER BY bucket_ts ASC",
                bucket_interval
            ))
            .bind::<diesel::sql_types::Text, _>(address)
            .bind::<diesel::sql_types::Text, _>(market_id)
            .bind::<diesel::sql_types::Timestamp, _>(start_time)
            .load::<UserPositionHistoryPoint>(&mut conn)
            .await?
        } else {
            // For all markets: pick last snapshot per (bucket, market), then SUM across markets.
            diesel::sql_query(format!(
                "SELECT bucket_ts as timestamp, \
                        SUM(supply_amount_usd) as supply_amount_usd, \
                        SUM(borrow_amount_usd) as borrow_amount_usd \
                 FROM ( \
                     SELECT DISTINCT ON (date_trunc('{0}', timestamp), market_id) \
                            date_trunc('{0}', timestamp) as bucket_ts, \
                            market_id, \
                            supply_amount_usd, \
                            borrow_amount_usd \
                     FROM lending_position_snapshots \
                     WHERE address = $1 AND timestamp >= $2 \
                     ORDER BY date_trunc('{0}', timestamp), market_id, timestamp DESC \
                 ) sub \
                 GROUP BY bucket_ts \
                 ORDER BY bucket_ts ASC",
                bucket_interval
            ))
            .bind::<diesel::sql_types::Text, _>(address)
            .bind::<diesel::sql_types::Timestamp, _>(start_time)
            .load::<UserPositionHistoryPoint>(&mut conn)
            .await?
        };

        Ok(results)
    }

    /******* Private helper methods *******/

    async fn calculate_user_position(&self, address: &str, market_id: &str) -> Result<Option<Position>> {
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

    async fn calculate_positions_for_market(&self, market_id: &str, page: i64, limit: i64) -> Result<Vec<Position>> {
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

        positions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));

        Ok(positions)
    }

    pub async fn get_position_snapshots_in_period(
        &self,
        start_time: chrono::NaiveDateTime,
        end_time: chrono::NaiveDateTime,
    ) -> Result<Vec<crate::models::PositionSnapshot>> {
        let mut conn = self.db_pool.get().await?;

        let snapshots: Vec<crate::models::PositionSnapshot> = schema::lending_position_snapshots::table
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

    /// SUM of the latest per-market USD totals across all markets. Reads from
    /// `market_state_snapshots` (one row per market per tick) — orders of magnitude
    /// fewer rows than aggregating per-user position snapshots.
    pub async fn get_latest_position_snapshot_totals(&self) -> Result<PositionTotals> {
        let mut conn = self.db_pool.get().await?;

        let row: PositionTotals = diesel::sql_query(
            "SELECT COALESCE(SUM(total_supply_usd), 0)     AS total_supply_usd, \
                    COALESCE(SUM(total_borrow_usd), 0)     AS total_borrow_usd, \
                    COALESCE(SUM(total_collateral_usd), 0) AS total_collateral_usd \
             FROM ( \
                 SELECT DISTINCT ON (market_id) \
                        market_id, total_supply_usd, total_borrow_usd, total_collateral_usd \
                 FROM market_state_snapshots \
                 ORDER BY market_id, snapshot_timestamp DESC \
             ) latest",
        )
        .get_result(&mut conn)
        .await?;

        Ok(row)
    }

    /// 30-day rolling average supply APY across markets, weighted by each market's current TVL.
    /// `supply_apy` is derived per row from stored `borrow_apy`, `total_*_assets`, and `fee`
    /// (WAD-scaled): `supply = borrow × utilization × (1 − fee/WAD)`.
    /// Per-market TVL = `total_supply_usd + total_collateral_usd` on the latest row.
    pub async fn get_30d_avg_apy_tvl_weighted(&self) -> Result<BigDecimal> {
        let mut conn = self.db_pool.get().await?;

        let row: Apy30d = diesel::sql_query(
            "WITH supply_rates AS ( \
                 SELECT market_id, \
                        CASE WHEN total_supply_assets > 0 \
                             THEN borrow_apy \
                                  * (total_borrow_assets / total_supply_assets) \
                                  * (1 - fee / 1000000000000000000) \
                             ELSE 0 \
                        END AS supply_apy \
                 FROM market_state_snapshots \
                 WHERE snapshot_timestamp >= NOW() - INTERVAL '30 days' \
             ), \
             market_avg AS ( \
                 SELECT market_id, AVG(supply_apy) AS avg_rate \
                 FROM supply_rates \
                 GROUP BY market_id \
             ), \
             market_tvl AS ( \
                 SELECT DISTINCT ON (market_id) \
                        market_id, total_supply_usd + total_collateral_usd AS tvl_usd \
                 FROM market_state_snapshots \
                 ORDER BY market_id, snapshot_timestamp DESC \
             ) \
             SELECT COALESCE(SUM(t.tvl_usd * a.avg_rate) / NULLIF(SUM(t.tvl_usd), 0), 0) AS apy_30d_avg \
             FROM market_avg a JOIN market_tvl t USING (market_id) \
             WHERE t.tvl_usd > 0",
        )
        .get_result(&mut conn)
        .await?;

        Ok(row.apy_30d_avg)
    }

    pub async fn insert_lending_stats_snapshot(&self, snapshot: &NewLendingStatsSnapshot) -> Result<()> {
        use schema::lending_stats_snapshots::dsl::*;
        let mut conn = self.db_pool.get().await?;
        diesel::insert_into(lending_stats_snapshots).values(snapshot).execute(&mut conn).await?;
        Ok(())
    }

    pub async fn get_latest_lending_stats(&self) -> Result<Option<LendingStatsSnapshot>> {
        use schema::lending_stats_snapshots::dsl::*;
        let mut conn = self.db_pool.get().await?;
        let row = lending_stats_snapshots
            .order(snapshot_timestamp.desc())
            .first::<LendingStatsSnapshot>(&mut conn)
            .await
            .optional()?;
        Ok(row)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewPositionSnapshot;
    use bigdecimal::BigDecimal;
    use chrono::{Duration, NaiveDate, Timelike, Utc};
    use diesel_async::AsyncPgConnection;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;
    use diesel_async::pooled_connection::bb8::Pool;
    use std::str::FromStr;

    async fn create_test_pool() -> Arc<Pool<AsyncPgConnection>> {
        dotenvy::dotenv().ok();

        let user = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
        let password = std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
        let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
        let db = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "bento_alephium".to_string());

        let database_url = format!("postgresql://{}:{}@{}:{}/{}", user, password, host, port, db);

        let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(&database_url);
        let pool = Pool::builder()
            .max_size(2)
            .build(config)
            .await
            .expect("Failed to create test DB pool. Is PostgreSQL running?");

        Arc::new(pool)
    }

    /// Clean up test data by exact address match.
    async fn cleanup_test_snapshots(pool: &Arc<Pool<AsyncPgConnection>>, address: &str) {
        let mut conn = pool.get().await.unwrap();
        diesel::sql_query("DELETE FROM lending_position_snapshots WHERE address = $1")
            .bind::<diesel::sql_types::Text, _>(address)
            .execute(&mut conn)
            .await
            .unwrap();
    }

    /// The stats aggregation queries scan the whole table (no address filter), so tests
    /// that exercise them need a clean slate across both snapshot tables.
    async fn cleanup_all_stats_tables(pool: &Arc<Pool<AsyncPgConnection>>) {
        let mut conn = pool.get().await.unwrap();
        diesel::sql_query("DELETE FROM lending_position_snapshots").execute(&mut conn).await.unwrap();
        diesel::sql_query("DELETE FROM market_state_snapshots").execute(&mut conn).await.unwrap();
    }

    fn make_snapshot(
        address: &str,
        market_id: &str,
        supply_usd: &str,
        borrow_usd: &str,
        timestamp: NaiveDateTime,
    ) -> NewPositionSnapshot {
        make_snapshot_full(address, market_id, supply_usd, borrow_usd, "0", timestamp)
    }

    fn make_snapshot_full(
        address: &str,
        market_id: &str,
        supply_usd: &str,
        borrow_usd: &str,
        collateral_usd: &str,
        timestamp: NaiveDateTime,
    ) -> NewPositionSnapshot {
        NewPositionSnapshot {
            address: address.to_string(),
            market_id: market_id.to_string(),
            supply_amount: BigDecimal::from(0),
            supply_amount_usd: BigDecimal::from_str(supply_usd).unwrap(),
            borrow_amount: BigDecimal::from(0),
            borrow_amount_usd: BigDecimal::from_str(borrow_usd).unwrap(),
            collateral_amount: BigDecimal::from(0),
            collateral_amount_usd: BigDecimal::from_str(collateral_usd).unwrap(),
            timestamp,
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_hourly_buckets() {
        let pool = create_test_pool().await;
        let test_addr = "test-hourly-buckets";
        cleanup_test_snapshots(&pool, test_addr).await;

        let repo = LendingRepository::new(pool.clone());
        let now = Utc::now().naive_utc();
        // Place snapshots 3 hours ago, well within the OneMonth (30-day) window.
        // Truncate to the start of the hour to avoid bucket boundary issues.
        let base = now - Duration::hours(3);
        let base = base.date().and_hms_opt(base.time().hour(), 0, 0).unwrap();

        // Two snapshots in hour 0, two in hour +1 → should produce 2 hourly buckets
        let snapshots = vec![
            make_snapshot(test_addr, "market_a", "100.0", "10.0", base),
            make_snapshot(test_addr, "market_a", "200.0", "20.0", base + Duration::minutes(30)),
            make_snapshot(test_addr, "market_a", "300.0", "30.0", base + Duration::hours(1)),
            make_snapshot(test_addr, "market_a", "500.0", "50.0", base + Duration::hours(1) + Duration::minutes(30)),
        ];
        repo.insert_position_snapshots(&snapshots).await.unwrap();

        // OneMonth uses hourly bucket_interval
        let result = repo
            .get_user_position_history(test_addr, Some("market_a"), crate::models::Timeframe::OneMonth)
            .await
            .unwrap();

        assert_eq!(result.len(), 2, "Expected 2 hourly buckets");

        // First bucket: last snapshot at base+30m → 200, 20
        assert_eq!(result[0].supply_amount_usd, BigDecimal::from_str("200").unwrap());
        assert_eq!(result[0].borrow_amount_usd, BigDecimal::from_str("20").unwrap());

        // Second bucket: last snapshot at base+1h30m → 500, 50
        assert_eq!(result[1].supply_amount_usd, BigDecimal::from_str("500").unwrap());
        assert_eq!(result[1].borrow_amount_usd, BigDecimal::from_str("50").unwrap());

        cleanup_test_snapshots(&pool, test_addr).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_daily_buckets() {
        let pool = create_test_pool().await;
        let test_addr = "test-daily-buckets";
        cleanup_test_snapshots(&pool, test_addr).await;

        let repo = LendingRepository::new(pool.clone());
        // Fixed past dates — All timeframe uses daily buckets and has no time cutoff
        let day1 = NaiveDate::from_ymd_opt(2025, 6, 10).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2025, 6, 11).unwrap().and_hms_opt(10, 0, 0).unwrap();

        // Two pairs of snapshots on different days → 2 daily buckets
        let snapshots = vec![
            make_snapshot(test_addr, "market_a", "100.0", "10.0", day1),
            make_snapshot(test_addr, "market_a", "200.0", "20.0", day1 + Duration::minutes(30)),
            make_snapshot(test_addr, "market_a", "300.0", "30.0", day2),
            make_snapshot(test_addr, "market_a", "500.0", "50.0", day2 + Duration::minutes(30)),
        ];
        repo.insert_position_snapshots(&snapshots).await.unwrap();

        let result =
            repo.get_user_position_history(test_addr, Some("market_a"), crate::models::Timeframe::All).await.unwrap();

        assert_eq!(result.len(), 2, "Expected 2 daily buckets");

        // Day 1 bucket: last snapshot at day1+30m → 200, 20
        assert_eq!(result[0].supply_amount_usd, BigDecimal::from_str("200").unwrap());
        assert_eq!(result[0].borrow_amount_usd, BigDecimal::from_str("20").unwrap());

        // Day 2 bucket: last snapshot at day2+30m → 500, 50
        assert_eq!(result[1].supply_amount_usd, BigDecimal::from_str("500").unwrap());
        assert_eq!(result[1].borrow_amount_usd, BigDecimal::from_str("50").unwrap());

        cleanup_test_snapshots(&pool, test_addr).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_multi_market_aggregation() {
        let pool = create_test_pool().await;
        let test_addr = "test-multi-market";
        cleanup_test_snapshots(&pool, test_addr).await;

        let repo = LendingRepository::new(pool.clone());
        let base = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap();

        // Two markets, same timestamp bucket
        let snapshots = vec![
            make_snapshot(test_addr, "market_a", "100.0", "10.0", base),
            make_snapshot(test_addr, "market_a", "200.0", "20.0", base + Duration::minutes(30)),
            make_snapshot(test_addr, "market_b", "50.0", "5.0", base),
            make_snapshot(test_addr, "market_b", "150.0", "15.0", base + Duration::minutes(30)),
        ];
        repo.insert_position_snapshots(&snapshots).await.unwrap();

        // Query without market_id → should SUM last snapshot per market
        let result = repo.get_user_position_history(test_addr, None, crate::models::Timeframe::All).await.unwrap();

        assert_eq!(result.len(), 1, "Expected 1 daily bucket");

        // market_a last = 200, market_b last = 150 → SUM = 350
        // market_a last borrow = 20, market_b last borrow = 15 → SUM = 35
        assert_eq!(result[0].supply_amount_usd, BigDecimal::from_str("350").unwrap());
        assert_eq!(result[0].borrow_amount_usd, BigDecimal::from_str("35").unwrap());

        cleanup_test_snapshots(&pool, test_addr).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_no_data_returns_empty() {
        let pool = create_test_pool().await;
        let repo = LendingRepository::new(pool);

        let result = repo
            .get_user_position_history("test-nonexistent-addr", None, crate::models::Timeframe::OneMonth)
            .await
            .unwrap();

        assert!(result.is_empty());
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_timeframe_filtering() {
        let pool = create_test_pool().await;
        let test_addr = "test-tf-filter";
        cleanup_test_snapshots(&pool, test_addr).await;

        let repo = LendingRepository::new(pool.clone());
        let now = Utc::now().naive_utc();

        // 5 days ago is well within OneMonth (30 days). 60 days ago is well outside.
        // The wide margins (25-day gap to the cutoff on both sides) make sub-second
        // timing differences between test setup and query execution irrelevant.
        let recent = now - Duration::days(5);
        let old = now - Duration::days(60);

        let snapshots = vec![
            make_snapshot(test_addr, "market_a", "100.0", "10.0", recent),
            make_snapshot(test_addr, "market_a", "999.0", "99.0", old),
        ];
        repo.insert_position_snapshots(&snapshots).await.unwrap();

        // OneMonth should only include the recent data point
        let result = repo
            .get_user_position_history(test_addr, Some("market_a"), crate::models::Timeframe::OneMonth)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].supply_amount_usd, BigDecimal::from_str("100").unwrap());

        // With All timeframe, should get both (different daily buckets)
        let result_all =
            repo.get_user_position_history(test_addr, Some("market_a"), crate::models::Timeframe::All).await.unwrap();
        assert_eq!(result_all.len(), 2);

        cleanup_test_snapshots(&pool, test_addr).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_single_snapshot_per_bucket() {
        let pool = create_test_pool().await;
        let test_addr = "test-single-snap";
        cleanup_test_snapshots(&pool, test_addr).await;

        let repo = LendingRepository::new(pool.clone());
        let base = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap().and_hms_opt(10, 15, 0).unwrap();

        let snapshots = vec![make_snapshot(test_addr, "market_a", "42.5", "7.3", base)];
        repo.insert_position_snapshots(&snapshots).await.unwrap();

        let result =
            repo.get_user_position_history(test_addr, Some("market_a"), crate::models::Timeframe::All).await.unwrap();

        assert_eq!(result.len(), 1);
        // Single snapshot in bucket — last value equals the only value
        assert_eq!(result[0].supply_amount_usd, BigDecimal::from_str("42.5").unwrap());
        assert_eq!(result[0].borrow_amount_usd, BigDecimal::from_str("7.3").unwrap());

        cleanup_test_snapshots(&pool, test_addr).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_get_latest_position_snapshot_totals_empty() {
        let pool = create_test_pool().await;
        cleanup_all_stats_tables(&pool).await;

        let repo = LendingRepository::new(pool.clone());
        let totals = repo.get_latest_position_snapshot_totals().await.unwrap();

        assert_eq!(totals.total_supply_usd, BigDecimal::from(0));
        assert_eq!(totals.total_borrow_usd, BigDecimal::from(0));
        assert_eq!(totals.total_collateral_usd, BigDecimal::from(0));
    }
}
