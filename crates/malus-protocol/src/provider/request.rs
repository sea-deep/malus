//! Provider RPC request types sent by `malusd` to provider child processes over stdio.

use crate::wire::{ActionRequestV0, LibraryKindWire, SearchKindWire, SurfaceCursorWire, TrackWire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ProviderRequest {
    Ping,
    Hello {
        version: (u32, u32),
    },
    GetCapabilities,
    GetSurfaceManifest,
    GetSurface {
        surface_id: String,
    },
    ContinueSurface {
        surface_id: String,
        cursor: SurfaceCursorWire,
    },
    InvokeSurfaceAction {
        invocation_token: String,
    },
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        kinds: Vec<SearchKindWire>,
        limit: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    GetCatalogItem {
        media_id: String,
    },
    GetCollectionItems {
        media_id: String,
        limit: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    GetLibrary {
        kind: LibraryKindWire,
        limit: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
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
    GetStatus,
    GetQueue,
    Enqueue {
        track: TrackWire,
    },
    Action(ActionRequestV0),
    GetAuthStatus,
    AuthBegin,
    AuthLogout,
    Shutdown,
}
