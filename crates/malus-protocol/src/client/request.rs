//! Client RPC request types sent by CLI / TUI / GUI frontends to `malusd` over IPC.

use crate::wire::{
    ActionRequestV0, AppleActionWire, LibraryKindWire, PageCursorWire, RepeatModeWire,
    SearchKindWire, TrackWire,
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
        provider: Option<String>,
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
        provider: Option<String>,
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
    GetCapabilities {
        provider: String,
    },
    ListProviders,
    GetAuthStatus {
        provider: String,
    },
    AuthBegin {
        provider: String,
    },
    AuthLogout {
        provider: String,
    },
    GetNavigation,
    GetPage {
        route: String,
    },
    ContinuePage {
        route: String,
        cursor: PageCursorWire,
    },
    InvokeAction {
        action: AppleActionWire,
    },
    GetProviderSurfaceManifest {
        #[serde(default)]
        provider: String,
    },
    GetSurface {
        #[serde(default)]
        provider: String,
        surface_id: String,
    },
    ContinueSurface {
        #[serde(default)]
        provider: String,
        surface_id: String,
        cursor: PageCursorWire,
    },
    InvokeSurfaceAction {
        #[serde(default)]
        provider: String,
        invocation_token: String,
    },
    Action(ActionRequestV0),
    SubscribeEvents,
}
