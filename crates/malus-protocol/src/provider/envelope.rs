//! Wire envelopes for multiplexed provider RPC and event streaming.

use serde::{Deserialize, Serialize};

use super::{ProviderEvent, ProviderRequest, ProviderResponse};

/// An identified request sent by `malusd` to a provider process over stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRequestEnvelope {
    pub id: u64,
    #[serde(flatten)]
    pub request: ProviderRequest,
}

impl ProviderRequestEnvelope {
    pub fn new(id: u64, request: ProviderRequest) -> Self {
        Self { id, request }
    }
}

/// A multiplexed message sent by a provider process to `malusd` over stdout.
///
/// Explicitly tagged with `"kind"` so that responses and asynchronous events
/// can share the standard output stream safely without ambiguity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ProviderWireMessage {
    #[serde(rename = "response")]
    Response {
        id: u64,
        #[serde(flatten)]
        response: ProviderResponse,
    },
    #[serde(rename = "event")]
    Event {
        #[serde(flatten)]
        event: ProviderEvent,
    },
}

impl ProviderWireMessage {
    pub fn response(id: u64, response: ProviderResponse) -> Self {
        Self::Response { id, response }
    }

    pub fn event(event: ProviderEvent) -> Self {
        Self::Event { event }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{PlaybackStateWire, PlayerStatusWire, RepeatModeWire};

    #[test]
    fn test_provider_request_envelope_roundtrip() {
        let envelope = ProviderRequestEnvelope::new(42, ProviderRequest::Ping);
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"method\":\"Ping\""));

        let decoded: ProviderRequestEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn test_provider_wire_message_response_roundtrip() {
        let msg = ProviderWireMessage::response(42, ProviderResponse::Pong);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"kind\":\"response\""));
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"type\":\"Pong\""));

        let decoded: ProviderWireMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_provider_wire_message_event_roundtrip() {
        let status = PlayerStatusWire {
            state: PlaybackStateWire::Playing,
            current_track: None,
            position_ms: 1000,
            duration_ms: 200000,
            volume: 80,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        };
        let msg = ProviderWireMessage::event(ProviderEvent::StatusChanged(status));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"kind\":\"event\""));
        assert!(json.contains("\"event\":\"StatusChanged\""));

        let decoded: ProviderWireMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, msg);
    }
}
