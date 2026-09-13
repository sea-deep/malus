//! Client RPC response types returned by `malusd` to frontends.

use crate::wire::{
    AppleNavigationWire, ApplePageWire, AuthStatusWire, CatalogItemWire, LibraryPageWire,
    PageContinuationWire, PageWire, PlayerStatusWire, ProviderInfoWire, QueueWire,
    SearchResultsWire, SurfaceActionResultWire, TrackWire,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientResponse {
    Pong,
    Status(PlayerStatusWire),
    SearchResults(SearchResultsWire),
    CatalogItem(CatalogItemWire),
    CollectionItems(PageWire<TrackWire>),
    LibraryPage(LibraryPageWire),
    Queue(QueueWire),
    Capabilities {
        provider: String,
        capabilities: Vec<String>,
    },
    Providers(Vec<ProviderInfoWire>),
    Navigation(AppleNavigationWire),
    Page(ApplePageWire),
    PageContinued(PageContinuationWire),
    ProviderSurfaceManifest(AppleNavigationWire),
    Surface(ApplePageWire),
    SurfaceContinued(PageContinuationWire),
    SurfaceActionResult(SurfaceActionResultWire),
    ActionResult(serde_json::Value),
    AuthStatus(AuthStatusWire),
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
