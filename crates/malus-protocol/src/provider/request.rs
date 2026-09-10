//! Provider RPC request types sent by `malusd` to provider child processes over stdio.

use crate::wire::{ActionRequestV0, TrackWire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ProviderRequest {
    Ping,
    Hello { version: (u32, u32) },
    GetCapabilities,
    Search { query: String, limit: usize },
    Play,
    PlayTrack { media_id: String },
    Pause,
    TogglePlay,
    Stop,
    Next,
    Previous,
    Seek { position_ms: u64 },
    SetVolume { volume: u8 },
    GetStatus,
    GetQueue,
    Enqueue { track: TrackWire },
    Action(ActionRequestV0),
    Shutdown,
}
