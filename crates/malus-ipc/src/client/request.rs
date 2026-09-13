//! Client RPC request types sent by CLI / TUI / GUI frontends to `malusd` over IPC.

use crate::wire::{
    LibraryKindWire, MediaRef, PageActionWire, PageCursorWire, PageRoute, RepeatMode,
    SearchKindWire,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ClientRequest {
    Ping,
    GetStatus,
    Play,
    PlayMedia {
        reference: MediaRef,
    },
    Pause,
    TogglePlay,
    Stop,
    Next,
    Previous,
    Seek {
        position_ms: u64,
    },
    SetVolume {
        volume: u8,
    },
    SetShuffle {
        shuffle: bool,
    },
    SetRepeat {
        repeat: RepeatMode,
    },
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        kinds: Vec<SearchKindWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    GetCatalogItem {
        reference: MediaRef,
    },
    GetCollectionItems {
        reference: MediaRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    GetLibrary {
        kind: LibraryKindWire,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    GetQueue,
    GetAuthStatus,
    AuthBegin,
    AuthLogout,
    GetNavigation,
    GetPage {
        route: PageRoute,
    },
    ContinuePage {
        route: PageRoute,
        cursor: PageCursorWire,
    },
    InvokeAction {
        action: PageActionWire,
    },
    SubscribeEvents,
}
