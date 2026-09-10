//! Provider asynchronous events streamed by provider child processes to `malusd`.

use crate::wire::{PlayerStatusWire, QueueWire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum ProviderEvent {
    StatusChanged(PlayerStatusWire),
    QueueChanged(QueueWire),
    NeedsAuth { message: String },
    Error { code: String, message: String },
}
