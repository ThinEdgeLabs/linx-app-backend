use std::collections::HashMap;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut processor_factories = HashMap::new();
    processor_factories.insert("lending".to_string(), lending_example::processor_factory());
    bento_cli::run_command(processor_factories, None, true).await?;
    Ok(())
}
