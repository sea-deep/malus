//! Malus Daemon executable for native Apple Music.

use malusd::{
    Engine, Server, default_socket_path,
    logger::{DEFAULT_MAX_LOG_SIZE, DualWriter, RotatingFile},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing::{error, info};

fn default_log_path() -> PathBuf {
    if let Ok(path) = std::env::var("MALUS_LOG") {
        return PathBuf::from(path);
    }
    if let Ok(home) = std::env::var("HOME") {
        let dir = PathBuf::from(home).join(".local/share/malus");
        let _ = std::fs::create_dir_all(&dir);
        return dir.join("malusd.log");
    }
    PathBuf::from("/tmp/malusd.log")
}

fn max_log_size() -> u64 {
    std::env::var("MALUS_LOG_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_LOG_SIZE)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_path = default_log_path();
    let max_size = max_log_size();
    let rotating = Arc::new(Mutex::new(RotatingFile::new(log_path.clone(), max_size)));

    let make_writer = {
        let rotating = rotating.clone();
        move || DualWriter::new(rotating.clone())
    };

    tracing_subscriber::fmt()
        .with_writer(make_writer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,wpe_stderr=warn,wpe_stdout=info")
            }),
        )
        .init();

    info!(
        "Logging daemon activity to {} (max {} MB total rotated)",
        log_path.display(),
        (max_size * 2) / (1024 * 1024)
    );

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
