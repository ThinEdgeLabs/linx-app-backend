use linx_indexer::{
    get_processor_factories,
    routers::{AccountTransactionsRouter, LendingRouter, PointsRouter},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Get processor factories from shared function
    let processor_factories = get_processor_factories();

    // Adding routes
    let router = AccountTransactionsRouter::register()
        .merge(LendingRouter::register())
        .merge(PointsRouter::register());

    bento_cli::run_command(processor_factories, Some(router), true).await?;

    Ok(())
}
