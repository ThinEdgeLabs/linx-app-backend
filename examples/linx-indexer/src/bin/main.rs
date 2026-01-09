use linx_indexer::{
    config::AppConfig,
    get_processor_factories,
    routers::{AccountTransactionsRouter, LendingRouter, PointsRouter},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Load application configuration from environment variables
    let app_config = AppConfig::from_env()?;
    let app_config = Some(Arc::new(app_config) as Arc<dyn bento_types::config::AppConfigTrait>);

    // Get processor factories from shared function
    let processor_factories = get_processor_factories();

    // Adding routes
    let router = AccountTransactionsRouter::register()
        .merge(LendingRouter::register())
        .merge(PointsRouter::register());

    bento_cli::run_command(processor_factories, Some(router), true, app_config).await?;

    Ok(())
}
