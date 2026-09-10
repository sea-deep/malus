//! Normalized wire data transfer objects (DTOs) for audio media and actions.
//!
//! Enforces physical serialization boundaries between `malus-core` domain models
//! and over-the-wire IPC representations.

use malus_core::{MediaId, PlaybackState, Player, Queue, RepeatMode, Track};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Wire representation of a namespaced media identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct MediaIdWire(String);

impl MediaIdWire {
    pub fn parse(s: &str) -> Result<Self, String> {
        MediaId::parse(s)
            .map(|id| Self(id.to_string()))
            .map_err(|e| e.to_string())
    }

    pub fn new(provider: &str, kind: &str, id: &str) -> Result<Self, String> {
        MediaId::new(provider, kind, id)
            .map(|id| Self(id.to_string()))
            .map_err(|e| e.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn provider(&self) -> &str {
        self.0.split(':').next().unwrap_or("")
    }

    pub fn kind(&self) -> &str {
        self.0.split(':').nth(1).unwrap_or("")
    }

    pub fn id(&self) -> &str {
        let prefix_len = self.provider().len() + 1 + self.kind().len() + 1;
        if prefix_len <= self.0.len() {
            &self.0[prefix_len..]
        } else {
            ""
        }
    }

    pub fn opaque(&self) -> &str {
        self.id()
    }
}

impl TryFrom<String> for MediaIdWire {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<MediaIdWire> for String {
    fn from(wire: MediaIdWire) -> Self {
        wire.0
    }
}

impl From<&MediaId> for MediaIdWire {
    fn from(id: &MediaId) -> Self {
        Self(id.to_string())
    }
}

impl From<MediaId> for MediaIdWire {
    fn from(id: MediaId) -> Self {
        Self(id.to_string())
    }
}

impl TryFrom<MediaIdWire> for MediaId {
    type Error = malus_core::MediaIdError;

    fn try_from(wire: MediaIdWire) -> Result<Self, Self::Error> {
        MediaId::parse(&wire.0)
    }
}

impl std::ops::Deref for MediaIdWire {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for MediaIdWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Wire representation of a track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackWire {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

impl TrackWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            album: None,
            duration_ms: None,
            uri: None,
        }
    }

    pub fn media_id(&self) -> Result<MediaId, malus_core::MediaIdError> {
        MediaId::parse(&self.id)
    }
}

impl From<&Track> for TrackWire {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id.to_string(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_ms: track.duration_ms,
            uri: track.uri.clone(),
        }
    }
}

impl From<Track> for TrackWire {
    fn from(track: Track) -> Self {
        Self::from(&track)
    }
}

impl TryFrom<TrackWire> for Track {
    type Error = malus_core::MediaIdError;

    fn try_from(wire: TrackWire) -> Result<Self, Self::Error> {
        let id = MediaId::parse(&wire.id)?;
        let mut track = Track::new(id, wire.title, wire.artist);
        track.album = wire.album;
        track.duration_ms = wire.duration_ms;
        track.uri = wire.uri;
        Ok(track)
    }
}

/// Wire representation of playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackStateWire {
    Stopped,
    Playing,
    Paused,
}

impl From<PlaybackState> for PlaybackStateWire {
    fn from(state: PlaybackState) -> Self {
        match state {
            PlaybackState::Stopped => Self::Stopped,
            PlaybackState::Playing => Self::Playing,
            PlaybackState::Paused => Self::Paused,
        }
    }
}

impl From<PlaybackStateWire> for PlaybackState {
    fn from(wire: PlaybackStateWire) -> Self {
        match wire {
            PlaybackStateWire::Stopped => Self::Stopped,
            PlaybackStateWire::Playing => Self::Playing,
            PlaybackStateWire::Paused => Self::Paused,
        }
    }
}

/// Wire representation of repeat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatModeWire {
    Off,
    Track,
    All,
}

impl From<RepeatMode> for RepeatModeWire {
    fn from(mode: RepeatMode) -> Self {
        match mode {
            RepeatMode::Off => Self::Off,
            RepeatMode::Track => Self::Track,
            RepeatMode::All => Self::All,
        }
    }
}

impl From<RepeatModeWire> for RepeatMode {
    fn from(wire: RepeatModeWire) -> Self {
        match wire {
            RepeatModeWire::Off => Self::Off,
            RepeatModeWire::Track => Self::Track,
            RepeatModeWire::All => Self::All,
        }
    }
}

/// Wire representation of authoritative player status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStatusWire {
    pub state: PlaybackStateWire,
    pub current_track: Option<TrackWire>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: RepeatModeWire,
}

impl From<&Player> for PlayerStatusWire {
    fn from(p: &Player) -> Self {
        Self {
            state: p.state.into(),
            current_track: p.current_track.as_ref().map(Into::into),
            position_ms: p.position_ms,
            duration_ms: p.duration_ms,
            volume: p.volume,
            muted: p.muted,
            shuffle: p.shuffle,
            repeat: p.repeat.into(),
        }
    }
}

/// Wire representation of an authoritative queue snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueWire {
    pub items: Vec<TrackWire>,
    pub current_index: Option<usize>,
}

impl From<&Queue> for QueueWire {
    fn from(q: &Queue) -> Self {
        Self {
            items: q.items().iter().map(Into::into).collect(),
            current_index: q.current_index(),
        }
    }
}

/// Normalized action request envelope.
///
/// Action semantics remain provider-specific, but the targeting and envelope structure
/// are normalized across all layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRequestV0 {
    pub provider: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<MediaIdWire>,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl ActionRequestV0 {
    pub fn new(
        provider: impl Into<String>,
        action: impl Into<String>,
        target: Option<MediaIdWire>,
        params: serde_json::Value,
    ) -> Self {
        Self {
            provider: provider.into(),
            action: action.into(),
            target,
            params,
        }
    }
}

/// Wire representation of provider status and registration metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInfoWire {
    pub id: String,
    pub name: String,
    pub state: String,
    pub capabilities: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_wire_conversion() {
        let id = MediaId::new("mock", "track", "1").unwrap();
        let track = Track::new(id, "Title", "Artist");
        let wire = TrackWire::from(&track);
        assert_eq!(wire.id, "mock:track:1");
        assert_eq!(wire.title, "Title");

        let back: Track = wire.try_into().unwrap();
        assert_eq!(back.id.as_str(), "mock:track:1");
    }

    #[test]
    fn test_media_id_wire_multi_colon_opaque() {
        let wire = MediaIdWire::parse("foo:track:some:weird:native:id").unwrap();
        assert_eq!(wire.provider(), "foo");
        assert_eq!(wire.kind(), "track");
        assert_eq!(wire.id(), "some:weird:native:id");
        assert_eq!(wire.opaque(), "some:weird:native:id");
    }

    #[test]
    fn test_action_request_v0_serialization() {
        let action = ActionRequestV0 {
            provider: "mock".into(),
            action: "mock.repost".into(),
            target: Some(MediaIdWire::parse("mock:track:3").unwrap()),
            params: serde_json::json!({}),
        };

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"target\":\"mock:track:3\""));
        assert!(json.contains("\"action\":\"mock.repost\""));

        let deserialized: ActionRequestV0 = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, action);
    }
}
