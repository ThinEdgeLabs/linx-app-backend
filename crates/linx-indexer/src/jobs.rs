use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

#[async_trait]
pub trait PeriodicJob: Send + Sync {
    fn name(&self) -> &'static str;
    fn interval(&self) -> Duration;
    async fn tick(&self) -> anyhow::Result<()>;
}

pub async fn run_job_forever(job: Arc<dyn PeriodicJob>) -> anyhow::Result<()> {
    let mut timer = tokio::time::interval(job.interval());
    tracing::info!(job = job.name(), interval_secs = job.interval().as_secs(), "starting periodic job");
    loop {
        timer.tick().await;
        if let Err(e) = job.tick().await {
            tracing::error!(job = job.name(), error = %e, "periodic job tick failed");
        }
    }
}
