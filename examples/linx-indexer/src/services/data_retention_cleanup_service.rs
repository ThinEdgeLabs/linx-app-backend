use crate::jobs::PeriodicJob;
use crate::repository::LendingRepository;
use anyhow::Result;
use async_trait::async_trait;
use bento_core::DbPool;
use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use std::sync::Arc;
use std::time::Duration;

const POSITION_SNAPSHOT_PROCESSOR: &str = "position_snapshot_cleanup";
const POSITION_SNAPSHOT_RETENTION_DAYS: i64 = 365;
const TICK_INTERVAL_SECS: u64 = 86_400;

pub struct DataRetentionCleanupService {
    lending_repository: LendingRepository,
}

impl DataRetentionCleanupService {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { lending_repository: LendingRepository::new(db_pool) }
    }

    pub async fn cleanup_position_snapshots(&self) -> Result<()> {
        // Anything strictly before this is older than the retention horizon.
        let cutoff_day = (Utc::now().naive_utc().date() - ChronoDuration::days(POSITION_SNAPSHOT_RETENTION_DAYS))
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid");

        let Some(start_day) = self.resolve_position_snapshot_start_day().await? else {
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

    async fn resolve_position_snapshot_start_day(&self) -> Result<Option<NaiveDateTime>> {
        if let Some(status) = self.lending_repository.get_processor_status(POSITION_SNAPSHOT_PROCESSOR).await? {
            return Ok(Some(millis_to_day(status.last_timestamp) + ChronoDuration::days(1)));
        }
        self.lending_repository.oldest_position_snapshot_day().await
    }
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
        // Each step is logged-but-not-fatal so future cleanup steps don't block on each other.
        if let Err(e) = self.cleanup_position_snapshots().await {
            tracing::error!(target: "data_retention_cleanup", error = %e, "position snapshot cleanup failed");
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
        repo.insert_position_snapshots(&[snapshot_at("u1", "m1", old), snapshot_at("u1", "m1", recent)])
            .await
            .unwrap();

        let service = DataRetentionCleanupService::new(pool.clone());
        service.cleanup_position_snapshots().await.unwrap();

        // Old (400d) is past the 365d horizon; recent (100d) is well within it.
        assert_eq!(count_snapshots(&pool).await, 1, "only the recent snapshot should remain");

        // High-water mark advanced to (cutoff - 1d).
        let status = repo.get_processor_status(POSITION_SNAPSHOT_PROCESSOR).await.unwrap().unwrap();
        let high_water = millis_to_day(status.last_timestamp);
        let expected_cutoff = (now.date() - ChronoDuration::days(POSITION_SNAPSHOT_RETENTION_DAYS))
            .and_hms_opt(0, 0, 0)
            .unwrap();
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
}
