//! Standalone Mock Audio Provider process executable.

use malus_provider_mock::MockProvider;
use malus_provider_sdk::serve;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(MockProvider::new());
    serve(provider).await?;
    Ok(())
}
