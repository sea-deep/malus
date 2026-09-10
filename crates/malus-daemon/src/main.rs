//! Malus Daemon executable.

use malus_daemon::{Engine, Server, default_socket_path, discover_providers};
use std::{path::PathBuf, sync::Arc};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut socket_path = default_socket_path();
    let mut explicit_provider_bin: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" || arg == "-s" {
            if let Some(val) = args.next() {
                socket_path = PathBuf::from(val);
            }
        } else if arg == "--provider" || arg == "-p" {
            if let Some(val) = args.next() {
                explicit_provider_bin = Some(PathBuf::from(val));
            }
        } else if arg == "--help" || arg == "-h" {
            println!("Usage: malus-daemon [--socket <path>] [--provider <binary_path>]");
            return Ok(());
        }
    }

    // Generic provider discovery: discovering does NOT spawn processes (installed != running)
    let discovered = discover_providers(explicit_provider_bin.as_deref());
    info!(
        "Discovered {} provider(s): {:?}",
        discovered.len(),
        discovered.keys().collect::<Vec<_>>()
    );

    let engine = Arc::new(Engine::with_discovered(discovered));
    let server = Server::new(&socket_path, engine.clone());

    info!("Starting Malus daemon on {}", socket_path.display());

    tokio::select! {
        res = server.run() => {
            if let Err(e) = res {
                error!("Daemon error: {e}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal. Gracefully shutting down providers...");
            engine.shutdown().await;
        }
    }

    Ok(())
}
