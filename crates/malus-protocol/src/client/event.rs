//! Client broadcast event types streamed by `malusd` to connected frontends.

use crate::wire::{PlayerStatusWire, QueueWire, TrackWire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum ClientEvent {
    StatusChanged(PlayerStatusWire),
    QueueChanged(QueueWire),
    TrackChanged(Option<TrackWire>),
    ProviderStateChanged { provider: String, state: String },
}
