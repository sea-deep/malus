//! Malus Daemon executable.

use malus_daemon::{Engine, ProviderProcess, Server, default_socket_path};
use std::{path::PathBuf, sync::Arc};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut socket_path = default_socket_path();
    let mut provider_bin: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" || arg == "-s" {
            if let Some(val) = args.next() {
                socket_path = PathBuf::from(val);
            }
        } else if arg == "--provider" || arg == "-p" {
            if let Some(val) = args.next() {
                provider_bin = Some(PathBuf::from(val));
            }
        } else if arg == "--help" || arg == "-h" {
            println!("Usage: malus-daemon [--socket <path>] [--provider <binary_path>]");
            return Ok(());
        }
    }

    let engine = Arc::new(Engine::new());

    // If a provider binary is specified or discovered, spawn it as child process
    let mock_bin = provider_bin.or_else(|| {
        std::env::current_exe().ok().and_then(|mut p| {
            p.pop();
            let candidate = p.join("malus-provider-mock");
            if candidate.exists() {
                Some(candidate)
            } else {
                None
            }
        })
    });

    if let Some(bin) = mock_bin {
        match ProviderProcess::spawn("mock", "Mock Audio Provider", &bin, &[]).await {
            Ok(proc) => {
                engine.register_provider(Arc::new(proc)).await;
            }
            Err(e) => {
                tracing::warn!("Failed to launch mock provider at {}: {e}", bin.display());
            }
        }
    }

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
