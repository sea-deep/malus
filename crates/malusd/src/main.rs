//! Malus Daemon executable for native Apple Music.

use malusd::{Engine, Server, default_socket_path};
use std::{path::PathBuf, sync::Arc};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut socket_path = default_socket_path();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" || arg == "-s" {
            if let Some(val) = args.next() {
                socket_path = PathBuf::from(val);
            }
        } else if arg == "--help" || arg == "-h" {
            println!("Usage: malusd [--socket <path>]");
            return Ok(());
        }
    }

    let engine = Arc::new(Engine::new());
    let server = Server::new(&socket_path, engine.clone());

    info!("Starting Malus daemon on {}", socket_path.display());

    tokio::select! {
        res = server.run() => {
            if let Err(e) = res {
                error!("Daemon error: {e}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal. Gracefully shutting down Apple Music engine...");
            engine.shutdown().await;
        }
    }

    Ok(())
}
