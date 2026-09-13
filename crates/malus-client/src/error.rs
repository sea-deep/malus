use malus_ipc::client::ClientResponse;
use malus_ipc::codec::FrameError;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Failed to connect to daemon at {0}: {1}")]
    ConnectionFailed(PathBuf, std::io::Error),
    #[error("Daemon disconnected")]
    Disconnected,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol error: {0}")]
    Protocol(#[from] FrameError),
    #[error("Request timed out after {0:?}")]
    Timeout(Duration),
    #[error("Daemon returned error: [{code}] {message}")]
    ServerError { code: String, message: String },
    #[error("Unexpected response: {0:?}")]
    UnexpectedResponse(Box<ClientResponse>),
}
