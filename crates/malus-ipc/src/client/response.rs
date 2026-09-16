//! Client RPC response types returned by `malusd` to frontends.

use crate::wire::{
    AccountMediaState, ActionResultWire, AuthStatusWire, CatalogItemWire, Credits, LibraryPageWire,
    Lyrics, NavigationWire, PageContinuationWire, PageWire, PagedListWire, PlayerStatus, Playlist,
    Queue, SearchResultsWire, Track,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientResponse {
    Pong,
    Status(PlayerStatus),
    SearchResults(SearchResultsWire),
    CatalogItem(CatalogItemWire),
    CollectionItems(PagedListWire<Track>),
    LibraryPage(LibraryPageWire),
    Queue(Queue),
    Navigation(NavigationWire),
    Page(PageWire),
    PageContinued(PageContinuationWire),
    ActionResult(ActionResultWire),
    AuthStatus(AuthStatusWire),
    Lyrics(Lyrics),
    Credits(Credits),
    MediaState(AccountMediaState),
    Playlist(Playlist),
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
