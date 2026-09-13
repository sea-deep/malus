//! Client RPC request types sent by CLI / TUI / GUI frontends to `malusd` over IPC.

use crate::wire::{
    LibraryKindWire, PageActionWire, PageCursorWire, RepeatModeWire, SearchKindWire, TrackWire,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ClientRequest {
    Ping,
    GetStatus,
    Play,
    PlayTrack {
        media_id: String,
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
        repeat: RepeatModeWire,
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
        media_id: String,
    },
    GetCollectionItems {
        media_id: String,
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
    Enqueue {
        track: TrackWire,
    },
    GetQueue,
    ClearQueue,
    GetAuthStatus,
    AuthBegin,
    AuthLogout,
    GetNavigation,
    GetPage {
        route: String,
    },
    ContinuePage {
        route: String,
        cursor: PageCursorWire,
    },
    InvokeAction {
        action: PageActionWire,
    },
    SubscribeEvents,
}
