use std::collections::HashMap;

use linx_indexer::{
    processors::{contract_call_processor, dex_processor, lending_processor, transfer_processor},
    routers::{AccountTransactionsRouter, LendingRouter},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Adding processor factories
    let mut processor_factories = HashMap::new();
    processor_factories.insert("transfers".to_string(), transfer_processor::processor_factory());
    processor_factories
        .insert("contract_calls".to_string(), contract_call_processor::processor_factory());
    processor_factories.insert("dex".to_string(), dex_processor::processor_factory());
    processor_factories.insert("lending".to_string(), lending_processor::processor_factory());

    // Adding routes
    let router = AccountTransactionsRouter::register().merge(LendingRouter::register());

    bento_cli::run_command(processor_factories, Some(router), true).await?;

    Ok(())
}
