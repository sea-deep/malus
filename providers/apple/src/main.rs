//! Apple Music provider standalone executable.

use malus_provider_apple::AppleProvider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let provider = Arc::new(AppleProvider::production());
    malus_provider_sdk::serve(provider).await?;

    Ok(())
}
