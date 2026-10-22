use crate::jobs::PeriodicJob;
use crate::models::NewLendingStatsSnapshot;
use crate::repository::LendingRepository;
use async_trait::async_trait;
use bento_core::DbPool;
use std::sync::Arc;
use std::time::Duration;

pub struct StatsSnapshotService {
    lending_repository: LendingRepository,
}

impl StatsSnapshotService {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { lending_repository: LendingRepository::new(db_pool) }
    }

    pub async fn generate_snapshot(&self) -> anyhow::Result<()> {
        let totals = self.lending_repository.get_latest_position_snapshot_totals().await?;
        let apy_30d = self.lending_repository.get_30d_avg_apy_tvl_weighted().await?;

        let tvl_usd = &totals.total_supply_usd + &totals.total_collateral_usd;

        let snapshot = NewLendingStatsSnapshot {
            total_supply_usd: totals.total_supply_usd.with_scale(2),
            total_borrow_usd: totals.total_borrow_usd.with_scale(2),
            total_collateral_usd: totals.total_collateral_usd.with_scale(2),
            tvl_usd: tvl_usd.with_scale(2),
            apy_30d_avg: apy_30d.with_scale(3),
            snapshot_timestamp: chrono::Utc::now().naive_utc(),
        };

        self.lending_repository.insert_lending_stats_snapshot(&snapshot).await?;
        tracing::info!(
            tvl_usd = %snapshot.tvl_usd,
            apy_30d_avg = %snapshot.apy_30d_avg,
            "Inserted lending stats snapshot"
        );
        Ok(())
    }
}

#[async_trait]
impl PeriodicJob for StatsSnapshotService {
    fn name(&self) -> &'static str {
        "lending-stats-snapshots"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(3600)
    }

    async fn tick(&self) -> anyhow::Result<()> {
        self.generate_snapshot().await
    }
}
