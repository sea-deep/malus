//! Provider RPC response types returned by provider child processes to `malusd`.

use crate::wire::{
    AuthStatusWire, CatalogItemWire, LibraryPageWire, PageWire, PlayerStatusWire, QueueWire,
    SearchResultsWire, TrackWire,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ProviderResponse {
    Pong,
    Hello {
        id: String,
        name: String,
        version: (u32, u32),
        capabilities: Vec<String>,
        status: String,
    },
    Capabilities(Vec<String>),
    SearchResults(SearchResultsWire),
    CatalogItem(CatalogItemWire),
    CollectionItems(PageWire<TrackWire>),
    LibraryPage(LibraryPageWire),
    Status(PlayerStatusWire),
    Queue(QueueWire),
    ActionResult(serde_json::Value),
    AuthStatus(AuthStatusWire),
    Ok,
    Error {
        code: String,
        message: String,
    },
}

impl ProviderResponse {
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}
