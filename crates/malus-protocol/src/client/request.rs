//! Client RPC request types sent by CLI / TUI / GUI frontends to `malusd` over IPC.

use crate::wire::{ActionRequestV0, RepeatModeWire, TrackWire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ClientRequest {
    Ping,
    GetStatus,
    Play,
    PlayTrack { media_id: String },
    Pause,
    TogglePlay,
    Stop,
    Next,
    Previous,
    Seek { position_ms: u64 },
    SetVolume { volume: u8 },
    SetShuffle { shuffle: bool },
    SetRepeat { repeat: RepeatModeWire },
    Search { query: String },
    Enqueue { track: TrackWire },
    GetQueue,
    ClearQueue,
    GetCapabilities { provider: String },
    ListProviders,
    GetAuthStatus { provider: String },
    AuthBegin { provider: String },
    AuthLogout { provider: String },
    Action(ActionRequestV0),
    SubscribeEvents,
}
