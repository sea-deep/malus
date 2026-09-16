//! Client broadcast event types streamed by `malusd` to connected frontends.

use crate::wire::{AccountMediaState, AuthStatusWire, PlayerStatus, Queue, Track};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum ClientEvent {
    StatusChanged(PlayerStatus),
    PlaybackError { source: String, message: String },
    QueueChanged(Queue),
    TrackChanged(Option<Track>),
    AuthChanged(AuthStatusWire),
    MediaStateChanged(AccountMediaState),
}
