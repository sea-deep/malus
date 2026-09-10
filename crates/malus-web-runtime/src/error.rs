//! Error types for `malus-web-runtime`.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebError {
    #[error("No compatible browser candidate discovered")]
    NoCompatibleBrowserFound,

    #[error("Explicit browser binary not found or not executable: {0}")]
    ExplicitBrowserNotFound(PathBuf),

    #[error("Failed to prepare profile directory: {0}")]
    Profile(String),

    #[error("Failed to launch browser process: {0}")]
    Launch(String),

    #[error("Browser process exited prematurely with code: {0:?}")]
    BrowserExited(Option<i32>),

    #[error("Timed out waiting for DevToolsActivePort at {0}")]
    PortTimeout(PathBuf),

    #[error("Failed to parse DevToolsActivePort: {0}")]
    InvalidPortFile(String),

    #[error("Failed to establish WebSocket connection: {0}")]
    Connection(String),

    #[error("Runtime disconnected: {0}")]
    Disconnected(String),

    #[error("CDP command failed: {0}")]
    Protocol(String),

    #[error("JavaScript evaluation threw an error: {0}")]
    Evaluation(String),

    #[error("Target error: {0}")]
    Target(String),

    #[error("Operation timed out: {0}")]
    Timeout(String),

    #[error("Internal runtime error: {0}")]
    Internal(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
