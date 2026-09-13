//! Unix domain socket IPC server for Malus Daemon with LSP Content-Length framing.

use crate::engine::Engine;
use malus_ipc::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    client::{ClientRequest, ClientResponse},
    codec::{FrameError, decode_message, read_frame, write_message},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;
use tokio::{
    net::{UnixListener, UnixStream},
    sync::broadcast,
};
use tracing::{debug, info, warn};

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Framing / Protocol error: {0}")]
    Protocol(#[from] FrameError),
    #[error("Address in use or failed to bind: {0}")]
    Bind(String),
}

/// Determine the default socket path for malus daemon.
pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("MALUS_SOCKET") {
        return PathBuf::from(path);
    }
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("malus.sock");
    }
    PathBuf::from("/tmp/malus.sock")
}

pub struct Server {
    socket_path: PathBuf,
    engine: Arc<Engine>,
}

impl Server {
    pub fn new(socket_path: impl AsRef<Path>, engine: Arc<Engine>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            engine,
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn engine(&self) -> Arc<Engine> {
        self.engine.clone()
    }

    /// Start the server and listen for incoming connections.
    pub async fn run(&self) -> Result<(), ServerError> {
        // Clean up stale socket if it exists
        if self.socket_path.exists() {
            let _ = tokio::fs::remove_file(&self.socket_path).await;
        }

        if let Some(parent) = self.socket_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| ServerError::Bind(format!("{}: {e}", self.socket_path.display())))?;

        info!("Malus daemon listening on {}", self.socket_path.display());

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let engine = self.engine.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, engine).await {
                            debug!("Connection finished: {e}");
                        }
                    });
                }
                Err(e) => {
                    warn!("Failed to accept connection: {e}");
                }
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

async fn handle_connection(stream: UnixStream, engine: Arc<Engine>) -> Result<(), ServerError> {
    let (mut reader, mut writer) = stream.into_split();
    let mut buf = Vec::new();

    while let Some(frame) = read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await? {
        let req: ClientRequest = match decode_message(&frame) {
            Ok(r) => r,
            Err(e) => {
                let err_res = ClientResponse::err("INVALID_REQUEST", e.to_string());
                write_message(&mut writer, &err_res).await?;
                continue;
            }
        };

        if req == ClientRequest::SubscribeEvents {
            // Client enters subscription mode
            let ok_res = ClientResponse::Ok;
            write_message(&mut writer, &ok_res).await?;

            let mut event_rx = engine.subscribe();
            loop {
                tokio::select! {
                    res = event_rx.recv() => {
                        match res {
                            Ok(event) => {
                                if write_message(&mut writer, &event).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                debug!("Event subscriber lagged by {skipped} events");
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                break;
                            }
                        }
                    }
                    read_res = read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES) => {
                        match read_res {
                            Ok(None) | Err(_) => break, // Client disconnected
                            Ok(Some(_)) => {} // Ignore any incoming requests in event stream mode
                        }
                    }
                }
            }
            break;
        }

        let res = engine.handle_request(req).await;
        write_message(&mut writer, &res).await?;
    }

    Ok(())
}
