use crate::jobs::PeriodicJob;
use crate::repository::LendingRepository;
use anyhow::Result;
use async_trait::async_trait;
use bento_core::DbPool;
use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use std::sync::Arc;
use std::time::Duration;

const PROCESSOR_NAME: &str = "position_snapshot_rollup";
const RAW_RETENTION_DAYS: i64 = 30;
const TICK_INTERVAL_SECS: u64 = 86_400;

pub struct PositionSnapshotRetentionService {
    lending_repository: LendingRepository,
}

impl PositionSnapshotRetentionService {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { lending_repository: LendingRepository::new(db_pool) }
    }

    pub async fn rollup(&self) -> Result<()> {
        // The first day still considered "raw" — anything strictly before this is eligible.
        let cutoff_day = (Utc::now().naive_utc().date() - ChronoDuration::days(RAW_RETENTION_DAYS))
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid");

        let Some(start_day) = self.resolve_start_day().await? else {
            tracing::info!(target: "position_snapshot_rollup", "no snapshots to roll up");
            return Ok(());
        };

        if start_day >= cutoff_day {
            tracing::debug!(
                target: "position_snapshot_rollup",
                ?start_day, ?cutoff_day, "no days eligible for rollup yet"
            );
            return Ok(());
        }

        let mut day = start_day;
        let mut total_deleted = 0usize;
        let mut days_processed = 0u32;

        while day < cutoff_day {
            let day_end = day + ChronoDuration::days(1);
            let deleted = self.lending_repository.rollup_position_snapshots_for_day(day, day_end).await?;
            total_deleted += deleted;
            days_processed += 1;

            // High-water mark = start-of-day of the most recently *completed* day.
            // resolve_start_day() adds one day to compute the next day to process.
            self.lending_repository.upsert_processor_status(PROCESSOR_NAME, day_to_millis(day)).await?;

            if deleted > 0 {
                tracing::info!(
                    target: "position_snapshot_rollup",
                    %day, deleted, "rolled up day"
                );
            }
            day = day_end;
        }

        tracing::info!(
            target: "position_snapshot_rollup",
            days_processed, total_deleted, "rollup tick complete"
        );
        Ok(())
    }

    /// First day to process this tick. Resumes from the day after the high-water mark
    /// when present; otherwise starts from the oldest snapshot in the table.
    async fn resolve_start_day(&self) -> Result<Option<NaiveDateTime>> {
        if let Some(status) = self.lending_repository.get_processor_status(PROCESSOR_NAME).await? {
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
impl PeriodicJob for PositionSnapshotRetentionService {
    fn name(&self) -> &'static str {
        "position-snapshot-rollup"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(TICK_INTERVAL_SECS)
    }

    async fn tick(&self) -> Result<()> {
        self.rollup().await
    }
}
