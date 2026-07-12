use crate::jobs::PeriodicJob;
use crate::repository::{ChainDataDeleted, LendingRepository};
use anyhow::Result;
use async_trait::async_trait;
use bento_core::DbPool;
use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use std::sync::Arc;
use std::time::Duration;

const POSITION_SNAPSHOT_PROCESSOR: &str = "position_snapshot_cleanup";
const POSITION_SNAPSHOT_RETENTION_DAYS: i64 = 365;

const CHAIN_DATA_PROCESSOR: &str = "chain_data_cleanup";
const CHAIN_DATA_RETENTION_DAYS: i64 = 90;

const ACCOUNT_TX_PROCESSOR: &str = "account_transaction_cleanup";
const ACCOUNT_TX_RETENTION_DAYS: i64 = 365;

const TICK_INTERVAL_SECS: u64 = 86_400;

pub struct DataRetentionCleanupService {
    lending_repository: LendingRepository,
}

impl DataRetentionCleanupService {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { lending_repository: LendingRepository::new(db_pool) }
    }

    pub async fn cleanup_position_snapshots(&self) -> Result<()> {
        let cutoff_day = retention_cutoff_day(POSITION_SNAPSHOT_RETENTION_DAYS);

        let oldest = self.lending_repository.oldest_position_snapshot_day().await?;
        let Some(start_day) = self.resolve_start_day(POSITION_SNAPSHOT_PROCESSOR, oldest).await? else {
            tracing::info!(target: "data_retention_cleanup", "no position snapshots to clean up");
            return Ok(());
        };

        if start_day >= cutoff_day {
            tracing::debug!(
                target: "data_retention_cleanup",
                ?start_day, ?cutoff_day, "no position-snapshot days eligible for cleanup yet"
            );
            return Ok(());
        }

        let mut day = start_day;
        let mut total_deleted = 0usize;
        let mut days_processed = 0u32;

        while day < cutoff_day {
            let day_end = day + ChronoDuration::days(1);
            let deleted = self.lending_repository.delete_position_snapshots_for_day(day, day_end).await?;
            total_deleted += deleted;
            days_processed += 1;

            // High-water mark = start-of-day of the most recently *completed* day.
            // resolve_position_snapshot_start_day() adds one day to compute the next day to process.
            self.lending_repository.upsert_processor_status(POSITION_SNAPSHOT_PROCESSOR, day_to_millis(day)).await?;

            if deleted > 0 {
                tracing::info!(
                    target: "data_retention_cleanup",
                    %day, deleted, "cleaned up position snapshot day"
                );
            }
            day = day_end;
        }

        tracing::info!(
            target: "data_retention_cleanup",
            days_processed, total_deleted, "position snapshot cleanup tick complete"
        );
        Ok(())
    }

    /// Cascade-delete raw chain data (events → transactions → blocks) older than the retention
    /// horizon, one block-time day at a time. A single high-water mark covers all three tables
    /// because they are deleted together as a unit.
    ///
    /// NOTE: this permanently removes historical blocks. The standalone `gap_detector` tool scans
    /// `blocks` from genesis by default (`--min-height=0`) and will otherwise report every pruned
    /// height as a gap and try to re-backfill it — run it with `--min-height` at/above the
    /// retention cutoff once this job is live.
    pub async fn cleanup_chain_data(&self) -> Result<()> {
        let cutoff_day = retention_cutoff_day(CHAIN_DATA_RETENTION_DAYS);

        let oldest = self.lending_repository.oldest_block_day().await?;
        let Some(start_day) = self.resolve_start_day(CHAIN_DATA_PROCESSOR, oldest).await? else {
            tracing::info!(target: "data_retention_cleanup", "no chain data to clean up");
            return Ok(());
        };

        if start_day >= cutoff_day {
            tracing::debug!(
                target: "data_retention_cleanup",
                ?start_day, ?cutoff_day, "no chain-data days eligible for cleanup yet"
            );
            return Ok(());
        }

        let mut day = start_day;
        let mut total = ChainDataDeleted::default();
        let mut days_processed = 0u32;

        while day < cutoff_day {
            let day_end = day + ChronoDuration::days(1);
            let deleted = self.lending_repository.delete_chain_data_for_day(day, day_end).await?;
            total.events += deleted.events;
            total.transactions += deleted.transactions;
            total.blocks += deleted.blocks;
            days_processed += 1;

            // High-water mark = start-of-day of the most recently *completed* day.
            self.lending_repository.upsert_processor_status(CHAIN_DATA_PROCESSOR, day_to_millis(day)).await?;

            if deleted.total() > 0 {
                tracing::info!(
                    target: "data_retention_cleanup",
                    %day, events = deleted.events, transactions = deleted.transactions, blocks = deleted.blocks,
                    "cleaned up chain-data day"
                );
            }
            day = day_end;
        }

        tracing::info!(
            target: "data_retention_cleanup",
            days_processed, events = total.events, transactions = total.transactions, blocks = total.blocks,
            "chain data cleanup tick complete"
        );
        Ok(())
    }

    /// Delete account transactions older than the retention horizon, one day at a time.
    pub async fn cleanup_account_transactions(&self) -> Result<()> {
        let cutoff_day = retention_cutoff_day(ACCOUNT_TX_RETENTION_DAYS);

        let oldest = self.lending_repository.oldest_account_transaction_day().await?;
        let Some(start_day) = self.resolve_start_day(ACCOUNT_TX_PROCESSOR, oldest).await? else {
            tracing::info!(target: "data_retention_cleanup", "no account transactions to clean up");
            return Ok(());
        };

        if start_day >= cutoff_day {
            tracing::debug!(
                target: "data_retention_cleanup",
                ?start_day, ?cutoff_day, "no account-transaction days eligible for cleanup yet"
            );
            return Ok(());
        }

        let mut day = start_day;
        let mut total_deleted = 0usize;
        let mut days_processed = 0u32;

        while day < cutoff_day {
            let day_end = day + ChronoDuration::days(1);
            let deleted = self.lending_repository.delete_account_transactions_for_day(day, day_end).await?;
            total_deleted += deleted;
            days_processed += 1;

            self.lending_repository.upsert_processor_status(ACCOUNT_TX_PROCESSOR, day_to_millis(day)).await?;

            if deleted > 0 {
                tracing::info!(
                    target: "data_retention_cleanup",
                    %day, deleted, "cleaned up account-transaction day"
                );
            }
            day = day_end;
        }

        tracing::info!(
            target: "data_retention_cleanup",
            days_processed, total_deleted, "account transaction cleanup tick complete"
        );
        Ok(())
    }

    /// Resolve the first day a cleanup pass should process for `processor`.
    ///
    /// If a high-water mark exists, resume at the day after the last completed one; otherwise
    /// fall back to `oldest_day` (the oldest row in the target table). Returns `None` only when
    /// there is no high-water mark and the table is empty.
    async fn resolve_start_day(
        &self,
        processor: &str,
        oldest_day: Option<NaiveDateTime>,
    ) -> Result<Option<NaiveDateTime>> {
        if let Some(status) = self.lending_repository.get_processor_status(processor).await? {
            return Ok(Some(millis_to_day(status.last_timestamp) + ChronoDuration::days(1)));
        }
        Ok(oldest_day)
    }
}

/// Midnight (UTC) of `today - retention_days`. Anything strictly before this is past the horizon.
fn retention_cutoff_day(retention_days: i64) -> NaiveDateTime {
    (Utc::now().naive_utc().date() - ChronoDuration::days(retention_days))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
}

fn day_to_millis(day: NaiveDateTime) -> i64 {
    day.and_utc().timestamp_millis()
}

fn millis_to_day(ms: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp_millis(ms).expect("processor_status millis must be valid").naive_utc()
}

#[async_trait]
impl PeriodicJob for DataRetentionCleanupService {
    fn name(&self) -> &'static str {
        "data-retention-cleanup"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(TICK_INTERVAL_SECS)
    }

    async fn tick(&self) -> Result<()> {
        // Each step is logged-but-not-fatal so cleanup steps don't block on each other.
        if let Err(e) = self.cleanup_position_snapshots().await {
            tracing::error!(target: "data_retention_cleanup", error = %e, "position snapshot cleanup failed");
        }
        if let Err(e) = self.cleanup_chain_data().await {
            tracing::error!(target: "data_retention_cleanup", error = %e, "chain data cleanup failed");
        }
        if let Err(e) = self.cleanup_account_transactions().await {
            tracing::error!(target: "data_retention_cleanup", error = %e, "account transaction cleanup failed");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewPositionSnapshot;
    use crate::test_helpers::create_test_pool;
    use bigdecimal::BigDecimal;
    use chrono::Duration as ChronoDuration;
    use diesel_async::AsyncPgConnection;
    use diesel_async::RunQueryDsl;
    use diesel_async::pooled_connection::bb8::Pool;

    /// Service-level tests run cleanup over the entire table, so they need a clean slate
    /// across the snapshot table and the processor_status row each test owns.
    async fn reset_state(pool: &Arc<Pool<AsyncPgConnection>>) {
        let mut conn = pool.get().await.unwrap();
        diesel::sql_query("DELETE FROM lending_position_snapshots").execute(&mut conn).await.unwrap();
        diesel::sql_query("DELETE FROM processor_status WHERE processor = $1")
            .bind::<diesel::sql_types::Text, _>(POSITION_SNAPSHOT_PROCESSOR)
            .execute(&mut conn)
            .await
            .unwrap();
    }

    fn snapshot_at(address: &str, market_id: &str, ts: NaiveDateTime) -> NewPositionSnapshot {
        NewPositionSnapshot {
            address: address.to_string(),
            market_id: market_id.to_string(),
            supply_amount: BigDecimal::from(0),
            supply_amount_usd: BigDecimal::from(0),
            borrow_amount: BigDecimal::from(0),
            borrow_amount_usd: BigDecimal::from(0),
            collateral_amount: BigDecimal::from(0),
            collateral_amount_usd: BigDecimal::from(0),
            timestamp: ts,
        }
    }

    async fn count_snapshots(pool: &Arc<Pool<AsyncPgConnection>>) -> i64 {
        #[derive(diesel::QueryableByName)]
        struct Row {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            n: i64,
        }
        let mut conn = pool.get().await.unwrap();
        let row: Row = diesel::sql_query("SELECT COUNT(*) AS n FROM lending_position_snapshots")
            .get_result(&mut conn)
            .await
            .unwrap();
        row.n
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_cleanup_deletes_old_days_keeps_recent() {
        let pool = create_test_pool().await;
        reset_state(&pool).await;

        let now = Utc::now().naive_utc();
        let old = now - ChronoDuration::days(400);
        let recent = now - ChronoDuration::days(100);

        let repo = LendingRepository::new(pool.clone());
        repo.insert_position_snapshots(&[snapshot_at("u1", "m1", old), snapshot_at("u1", "m1", recent)]).await.unwrap();

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_position_snapshots().await.unwrap();

        // Old (400d) is past the 365d horizon; recent (100d) is well within it.
        assert_eq!(count_snapshots(&pool).await, 1, "only the recent snapshot should remain");

        // High-water mark advanced to (cutoff - 1d).
        let status = repo.get_processor_status(POSITION_SNAPSHOT_PROCESSOR).await.unwrap().unwrap();
        let high_water = millis_to_day(status.last_timestamp);
        let expected_cutoff =
            (now.date() - ChronoDuration::days(POSITION_SNAPSHOT_RETENTION_DAYS)).and_hms_opt(0, 0, 0).unwrap();
        assert_eq!(high_water, expected_cutoff - ChronoDuration::days(1));

        reset_state(&pool).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_cleanup_idempotent_on_rerun() {
        let pool = create_test_pool().await;
        reset_state(&pool).await;

        let now = Utc::now().naive_utc();
        let repo = LendingRepository::new(pool.clone());
        repo.insert_position_snapshots(&[
            snapshot_at("u1", "m1", now - ChronoDuration::days(400)),
            snapshot_at("u1", "m1", now - ChronoDuration::days(100)),
        ])
        .await
        .unwrap();

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_position_snapshots().await.unwrap();
        let after_first = count_snapshots(&pool).await;

        // Second run: high-water sits at cutoff - 1d, so start_day >= cutoff_day → early return.
        service.cleanup_position_snapshots().await.unwrap();
        let after_second = count_snapshots(&pool).await;
        assert_eq!(after_first, after_second, "rerun must not delete additional rows");

        reset_state(&pool).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_cleanup_empty_table_is_noop() {
        let pool = create_test_pool().await;
        reset_state(&pool).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_position_snapshots().await.unwrap();

        let repo = LendingRepository::new(pool);
        // No data → no high-water mark created.
        assert!(repo.get_processor_status(POSITION_SNAPSHOT_PROCESSOR).await.unwrap().is_none());
    }

    // ---- chain-data cascade cleanup ------------------------------------------------------

    /// Chain-data tests operate over the whole blocks/transactions/events tables, so reset all
    /// three plus the high-water mark this suite owns.
    async fn reset_chain_data(pool: &Arc<Pool<AsyncPgConnection>>) {
        let mut conn = pool.get().await.unwrap();
        // Order matters only for readability here; there are no FK constraints between them.
        diesel::sql_query("DELETE FROM events").execute(&mut conn).await.unwrap();
        diesel::sql_query("DELETE FROM transactions").execute(&mut conn).await.unwrap();
        diesel::sql_query("DELETE FROM blocks").execute(&mut conn).await.unwrap();
        diesel::sql_query("DELETE FROM processor_status WHERE processor = $1")
            .bind::<diesel::sql_types::Text, _>(CHAIN_DATA_PROCESSOR)
            .execute(&mut conn)
            .await
            .unwrap();
    }

    /// Insert a block plus one transaction and one event hanging off it, timestamped at `ts`.
    /// `n` disambiguates hashes/heights so multiple blocks can coexist.
    async fn insert_block_with_child(pool: &Arc<Pool<AsyncPgConnection>>, n: i64, ts: NaiveDateTime) {
        let mut conn = pool.get().await.unwrap();
        let block_hash = format!("block-{n}");
        let tx_hash = format!("tx-{n}");
        let event_id = format!("event-{n}");

        diesel::sql_query(
            "INSERT INTO blocks (hash, timestamp, chain_from, chain_to, height, nonce, version, \
             dep_state_hash, txs_hash, tx_number, target) \
             VALUES ($1, $2, 0, 0, $3, 'n', 'v', 'dsh', 'txsh', 0, 't')",
        )
        .bind::<diesel::sql_types::Text, _>(&block_hash)
        .bind::<diesel::sql_types::Timestamp, _>(ts)
        .bind::<diesel::sql_types::BigInt, _>(n)
        .execute(&mut conn)
        .await
        .unwrap();

        diesel::sql_query(
            "INSERT INTO transactions (tx_hash, unsigned, script_execution_ok, contract_inputs, \
             generated_outputs, input_signatures, script_signatures, block_hash) \
             VALUES ($1, '{}'::jsonb, true, '[]'::jsonb, '[]'::jsonb, ARRAY[]::TEXT[], ARRAY[]::TEXT[], $2)",
        )
        .bind::<diesel::sql_types::Text, _>(&tx_hash)
        .bind::<diesel::sql_types::Text, _>(&block_hash)
        .execute(&mut conn)
        .await
        .unwrap();

        diesel::sql_query(
            "INSERT INTO events (id, tx_id, contract_address, event_index, fields) \
             VALUES ($1, $2, 'c', 0, '{}'::jsonb)",
        )
        .bind::<diesel::sql_types::Text, _>(&event_id)
        .bind::<diesel::sql_types::Text, _>(&tx_hash)
        .execute(&mut conn)
        .await
        .unwrap();
    }

    async fn count_table(pool: &Arc<Pool<AsyncPgConnection>>, table: &str) -> i64 {
        #[derive(diesel::QueryableByName)]
        struct Row {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            n: i64,
        }
        let mut conn = pool.get().await.unwrap();
        // `table` is a hardcoded test constant, never user input.
        let row: Row =
            diesel::sql_query(format!("SELECT COUNT(*) AS n FROM {table}")).get_result(&mut conn).await.unwrap();
        row.n
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_chain_data_cascade_deletes_old_keeps_recent() {
        let pool = create_test_pool().await;
        reset_chain_data(&pool).await;

        let now = Utc::now().naive_utc();
        // Chain-data horizon is 90 days: 120d is past it, 10d is well within.
        insert_block_with_child(&pool, 1, now - ChronoDuration::days(120)).await;
        insert_block_with_child(&pool, 2, now - ChronoDuration::days(10)).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_chain_data().await.unwrap();

        // The old block and its transaction AND event are all cascade-deleted; recent survive.
        assert_eq!(count_table(&pool, "blocks").await, 1, "only the recent block should remain");
        assert_eq!(count_table(&pool, "transactions").await, 1, "old block's transaction should be gone");
        assert_eq!(count_table(&pool, "events").await, 1, "old block's event should be cascade-deleted");

        // High-water mark advanced to (cutoff - 1d).
        let repo = LendingRepository::new(pool.clone());
        let status = repo.get_processor_status(CHAIN_DATA_PROCESSOR).await.unwrap().unwrap();
        let high_water = millis_to_day(status.last_timestamp);
        let expected_cutoff = retention_cutoff_day(CHAIN_DATA_RETENTION_DAYS);
        assert_eq!(high_water, expected_cutoff - ChronoDuration::days(1));

        reset_chain_data(&pool).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_chain_data_cleanup_idempotent_on_rerun() {
        let pool = create_test_pool().await;
        reset_chain_data(&pool).await;

        let now = Utc::now().naive_utc();
        insert_block_with_child(&pool, 1, now - ChronoDuration::days(120)).await;
        insert_block_with_child(&pool, 2, now - ChronoDuration::days(10)).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_chain_data().await.unwrap();
        let after_first = (
            count_table(&pool, "blocks").await,
            count_table(&pool, "transactions").await,
            count_table(&pool, "events").await,
        );

        // Second run: high-water sits at cutoff - 1d, so start_day >= cutoff_day → early return.
        service.cleanup_chain_data().await.unwrap();
        let after_second = (
            count_table(&pool, "blocks").await,
            count_table(&pool, "transactions").await,
            count_table(&pool, "events").await,
        );
        assert_eq!(after_first, after_second, "rerun must not delete additional rows");

        reset_chain_data(&pool).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_chain_data_cleanup_empty_is_noop() {
        let pool = create_test_pool().await;
        reset_chain_data(&pool).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_chain_data().await.unwrap();

        let repo = LendingRepository::new(pool);
        assert!(repo.get_processor_status(CHAIN_DATA_PROCESSOR).await.unwrap().is_none());
    }

    // ---- account_transactions cleanup ----------------------------------------------------

    async fn reset_account_transactions(pool: &Arc<Pool<AsyncPgConnection>>) {
        let mut conn = pool.get().await.unwrap();
        diesel::sql_query("DELETE FROM account_transactions").execute(&mut conn).await.unwrap();
        diesel::sql_query("DELETE FROM processor_status WHERE processor = $1")
            .bind::<diesel::sql_types::Text, _>(ACCOUNT_TX_PROCESSOR)
            .execute(&mut conn)
            .await
            .unwrap();
    }

    /// Insert one account transaction at `ts`. `n` keeps `tx_id` (and thus the generated `tx_key`)
    /// unique so rows don't collide on the idempotency constraint.
    async fn insert_account_transaction(pool: &Arc<Pool<AsyncPgConnection>>, n: i64, ts: NaiveDateTime) {
        let mut conn = pool.get().await.unwrap();
        diesel::sql_query(
            "INSERT INTO account_transactions (address, tx_type, tx_id, from_group, to_group, \
             block_height, timestamp, details) \
             VALUES ('addr', 'transfer', $1, 0, 0, 0, $2, '{}'::jsonb)",
        )
        .bind::<diesel::sql_types::Text, _>(format!("atx-{n}"))
        .bind::<diesel::sql_types::Timestamp, _>(ts)
        .execute(&mut conn)
        .await
        .unwrap();
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_account_transactions_delete_old_keep_recent() {
        let pool = create_test_pool().await;
        reset_account_transactions(&pool).await;

        let now = Utc::now().naive_utc();
        // account_transactions horizon is 365 days: 400d is past it, 100d is within.
        insert_account_transaction(&pool, 1, now - ChronoDuration::days(400)).await;
        insert_account_transaction(&pool, 2, now - ChronoDuration::days(100)).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_account_transactions().await.unwrap();

        assert_eq!(count_table(&pool, "account_transactions").await, 1, "only the recent row should remain");

        let repo = LendingRepository::new(pool.clone());
        let status = repo.get_processor_status(ACCOUNT_TX_PROCESSOR).await.unwrap().unwrap();
        let high_water = millis_to_day(status.last_timestamp);
        let expected_cutoff = retention_cutoff_day(ACCOUNT_TX_RETENTION_DAYS);
        assert_eq!(high_water, expected_cutoff - ChronoDuration::days(1));

        reset_account_transactions(&pool).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_account_transactions_cleanup_idempotent_on_rerun() {
        let pool = create_test_pool().await;
        reset_account_transactions(&pool).await;

        let now = Utc::now().naive_utc();
        insert_account_transaction(&pool, 1, now - ChronoDuration::days(400)).await;
        insert_account_transaction(&pool, 2, now - ChronoDuration::days(100)).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_account_transactions().await.unwrap();
        let after_first = count_table(&pool, "account_transactions").await;

        service.cleanup_account_transactions().await.unwrap();
        let after_second = count_table(&pool, "account_transactions").await;
        assert_eq!(after_first, after_second, "rerun must not delete additional rows");

        reset_account_transactions(&pool).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires database"]
    async fn test_account_transactions_cleanup_empty_is_noop() {
        let pool = create_test_pool().await;
        reset_account_transactions(&pool).await;

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_account_transactions().await.unwrap();

        let repo = LendingRepository::new(pool);
        assert!(repo.get_processor_status(ACCOUNT_TX_PROCESSOR).await.unwrap().is_none());
    }
}
