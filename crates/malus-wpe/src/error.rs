//! Error types for `malus-wpe`.

use thiserror::Error;

use crate::discovery::BrowserEngine;

#[derive(Debug, Error)]
pub enum WebError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Failed to initialize {engine} engine: {message}")]
    Initialization {
        engine: BrowserEngine,
        message: String,
    },

    #[error("No compatible browser candidate discovered")]
    NoCompatibleBrowserFound,

    #[error("Failed to prepare profile directory: {0}")]
    Profile(String),

    #[error("Failed to launch browser process: {0}")]
    Launch(String),

    #[error("Browser process exited prematurely with code: {0:?}")]
    BrowserExited(Option<i32>),

    #[error("Failed to establish connection: {0}")]
    Connection(String),

    #[error("Runtime disconnected: {0}")]
    Disconnected(String),

    #[error("JavaScript evaluation threw an error: {0}")]
    Evaluation(String),

    #[error("Target error: {0}")]
    Target(String),

    #[error("Operation timed out: {0}")]
    Timeout(String),

    #[error("Internal runtime error: {0}")]
    Internal(String),

    #[error(
        "Widevine CDM was not found. Apple Music playback requires Widevine. Set MALUS_WIDEVINE_PATH to an existing libwidevinecdm.so or run 'malus setup-widevine'."
    )]
    WidevineNotFound,

    #[error(transparent)]
    Widevine(#[from] crate::widevine::WidevineError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
