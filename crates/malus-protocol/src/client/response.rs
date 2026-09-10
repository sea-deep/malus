//! Client RPC response types returned by `malusd` to frontends.

use crate::wire::{PlayerStatusWire, ProviderInfoWire, QueueWire, TrackWire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientResponse {
    Pong,
    Status(PlayerStatusWire),
    SearchResults {
        tracks: Vec<TrackWire>,
    },
    Queue(QueueWire),
    Capabilities {
        provider: String,
        capabilities: Vec<String>,
    },
    Providers(Vec<ProviderInfoWire>),
    ActionResult(serde_json::Value),
    Ok,
    Error {
        code: String,
        message: String,
    },
}

impl ClientResponse {
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}
