//! Client RPC response types returned by `malusd` to frontends.

use crate::wire::{
    ActionResultWire, AuthStatusWire, CatalogItemWire, LibraryPageWire, NavigationWire,
    PageContinuationWire, PageWire, PagedListWire, PlayerStatusWire, QueueWire, SearchResultsWire,
    TrackWire,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientResponse {
    Pong,
    Status(PlayerStatusWire),
    SearchResults(SearchResultsWire),
    CatalogItem(CatalogItemWire),
    CollectionItems(PagedListWire<TrackWire>),
    LibraryPage(LibraryPageWire),
    Queue(QueueWire),
    Navigation(NavigationWire),
    Page(PageWire),
    PageContinued(PageContinuationWire),
    ActionResult(ActionResultWire),
    AuthStatus(AuthStatusWire),
    Ok,
    Error { code: String, message: String },
}

impl ClientResponse {
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}
