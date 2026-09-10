//! Behavioral Mock Audio Provider implementation.
//!
//! Conforms to Malus behavioral provider architecture:
//! - Strictly namespaced IDs (`mock:track:1`, `mock:album:1`, `mock:artist:1`)
//! - Monotonic playback clock (`std::time::Instant`) advancing during playback
//! - Pausing freezes position; seeking updates clock base
//! - Authoritative internal playback queue
//! - Declares capabilities (`search`, `playback`, `playback.seek`, `queue.read`, `queue.edit`, `mock.repost`)
//! - Exposes custom action `mock.repost` via generic RPC
//! - Excludes `lyrics` capability

use async_trait::async_trait;
use malus_provider_sdk::{
    MediaIdWire, PlaybackStateWire, PlayerStatusWire, QueueWire, RepeatModeWire, TrackWire,
    capability, error::ProviderError, traits::Provider,
};
use std::{sync::Arc, time::Instant};
use tokio::sync::Mutex;

struct MockState {
    tracks: Vec<TrackWire>,
    queue: Vec<TrackWire>,
    current_index: Option<usize>,
    playback_state: PlaybackStateWire,
    position_base_ms: u64,
    last_started_at: Option<Instant>,
    volume: u8,
}

impl MockState {
    fn new() -> Self {
        let tracks = vec![
            TrackWire {
                id: "mock:track:1".into(),
                title: "Mock Track 1".into(),
                artist: "Mock Artist".into(),
                album: Some("Mock Album A".into()),
                duration_ms: Some(180_000),
                uri: Some("mock://tracks/1".into()),
            },
            TrackWire {
                id: "mock:track:2".into(),
                title: "Mock Track 2".into(),
                artist: "Mock Artist".into(),
                album: Some("Mock Album A".into()),
                duration_ms: Some(210_000),
                uri: Some("mock://tracks/2".into()),
            },
            TrackWire {
                id: "mock:track:3".into(),
                title: "Acoustic Sunset".into(),
                artist: "Solaris".into(),
                album: Some("Mock Album B".into()),
                duration_ms: Some(245_000),
                uri: Some("mock://tracks/3".into()),
            },
        ];

        Self {
            queue: tracks.clone(),
            current_index: Some(0),
            tracks,
            playback_state: PlaybackStateWire::Stopped,
            position_base_ms: 0,
            last_started_at: None,
            volume: 100,
        }
    }

    fn current_track(&self) -> Option<&TrackWire> {
        self.current_index.and_then(|i| self.queue.get(i))
    }

    fn current_position_ms(&self) -> u64 {
        let duration = self
            .current_track()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0);
        let pos = match (self.playback_state, self.last_started_at) {
            (PlaybackStateWire::Playing, Some(start)) => {
                self.position_base_ms + (start.elapsed().as_millis() as u64)
            }
            _ => self.position_base_ms,
        };
        if duration > 0 { pos.min(duration) } else { pos }
    }
}

pub struct MockProvider {
    state: Arc<Mutex<MockState>>,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::new())),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn name(&self) -> &str {
        "Mock Audio Provider"
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            capability::SEARCH.into(),
            capability::PLAYBACK.into(),
            capability::PLAYBACK_SEEK.into(),
            capability::QUEUE_READ.into(),
            capability::QUEUE_EDIT.into(),
            "mock.repost".into(),
        ]
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<TrackWire>, ProviderError> {
        let state = self.state.lock().await;
        let q = query.to_lowercase();
        let matches = state
            .tracks
            .iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&q)
                    || t.artist.to_lowercase().contains(&q)
                    || t.id.to_lowercase().contains(&q)
            })
            .take(limit)
            .cloned()
            .collect();
        Ok(matches)
    }

    async fn play(&self, media_id: &str) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let found_index = state.queue.iter().position(|t| t.id == media_id);

        let idx = if let Some(i) = found_index {
            i
        } else if let Some(track) = state.tracks.iter().find(|t| t.id == media_id).cloned() {
            state.queue.push(track);
            state.queue.len() - 1
        } else {
            return Err(ProviderError::NotFound(media_id.to_string()));
        };

        state.current_index = Some(idx);
        state.position_base_ms = 0;
        state.last_started_at = Some(Instant::now());
        state.playback_state = PlaybackStateWire::Playing;
        Ok(())
    }

    async fn pause(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let current_pos = state.current_position_ms();
        state.position_base_ms = current_pos;
        state.last_started_at = None;
        state.playback_state = PlaybackStateWire::Paused;
        Ok(())
    }

    async fn resume(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.current_index.is_none() && !state.queue.is_empty() {
            state.current_index = Some(0);
        }
        if state.current_index.is_some() {
            state.last_started_at = Some(Instant::now());
            state.playback_state = PlaybackStateWire::Playing;
            Ok(())
        } else {
            Err(ProviderError::Playback("Queue is empty".into()))
        }
    }

    async fn stop(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        state.position_base_ms = 0;
        state.last_started_at = None;
        state.playback_state = PlaybackStateWire::Stopped;
        Ok(())
    }

    async fn next(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.queue.is_empty() {
            return Err(ProviderError::Playback("Queue is empty".into()));
        }

        let next_idx = match state.current_index {
            Some(i) if i + 1 < state.queue.len() => Some(i + 1),
            _ => None,
        };

        state.current_index = next_idx;
        state.position_base_ms = 0;
        if state.playback_state == PlaybackStateWire::Playing {
            state.last_started_at = Some(Instant::now());
        }
        Ok(())
    }

    async fn previous(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.queue.is_empty() {
            return Err(ProviderError::Playback("Queue is empty".into()));
        }

        let prev_idx = match state.current_index {
            Some(i) if i > 0 => i - 1,
            _ => 0,
        };

        state.current_index = Some(prev_idx);
        state.position_base_ms = 0;
        if state.playback_state == PlaybackStateWire::Playing {
            state.last_started_at = Some(Instant::now());
        }
        Ok(())
    }

    async fn seek(&self, position_ms: u64) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let duration = state
            .current_track()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0);
        let clamped = if duration > 0 {
            position_ms.min(duration)
        } else {
            position_ms
        };

        state.position_base_ms = clamped;
        if state.playback_state == PlaybackStateWire::Playing {
            state.last_started_at = Some(Instant::now());
        }
        Ok(())
    }

    async fn set_volume(&self, volume: u8) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        state.volume = volume.min(100);
        Ok(())
    }

    async fn get_status(&self) -> Result<PlayerStatusWire, ProviderError> {
        let state = self.state.lock().await;
        let current_track = state.current_track().cloned();
        let duration_ms = current_track
            .as_ref()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0);
        let pos_ms = state.current_position_ms();

        Ok(PlayerStatusWire {
            state: state.playback_state,
            current_track,
            position_ms: pos_ms,
            duration_ms,
            volume: state.volume,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        })
    }

    async fn get_queue(&self) -> Result<QueueWire, ProviderError> {
        let state = self.state.lock().await;
        Ok(QueueWire {
            items: state.queue.clone(),
            current_index: state.current_index,
        })
    }

    async fn enqueue(&self, track: TrackWire) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let was_empty = state.queue.is_empty();
        state.queue.push(track);
        if was_empty {
            state.current_index = Some(0);
        }
        Ok(())
    }

    async fn custom_action(
        &self,
        action: &str,
        target: Option<&MediaIdWire>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let _ = params;
        if action == "mock.repost" {
            let target_str = target.map(|t| t.as_str());
            Ok(serde_json::json!({
                "reposted": true,
                "track_id": target_str,
                "provider": "mock",
            }))
        } else {
            Err(ProviderError::NotSupported(format!(
                "Action '{action}' is not supported"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_mock_provider_monotonic_clock_advancement() {
        let p = MockProvider::new();
        p.play("mock:track:1").await.unwrap();

        let s1 = p.get_status().await.unwrap();
        assert_eq!(s1.state, PlaybackStateWire::Playing);
        let pos1 = s1.position_ms;

        // Sleep 60ms
        sleep(Duration::from_millis(60)).await;

        let s2 = p.get_status().await.unwrap();
        let pos2 = s2.position_ms;
        assert!(
            pos2 >= pos1 + 50,
            "Playback clock should advance while playing"
        );

        // Pause
        p.pause().await.unwrap();
        let s3 = p.get_status().await.unwrap();
        assert_eq!(s3.state, PlaybackStateWire::Paused);
        let pos3 = s3.position_ms;

        // Sleep while paused
        sleep(Duration::from_millis(50)).await;
        let s4 = p.get_status().await.unwrap();
        assert_eq!(
            s4.position_ms, pos3,
            "Position must not advance while paused"
        );
    }

    #[tokio::test]
    async fn test_mock_provider_seek() {
        let p = MockProvider::new();
        p.play("mock:track:1").await.unwrap();
        p.seek(50_000).await.unwrap();

        let s = p.get_status().await.unwrap();
        assert!(s.position_ms >= 50_000);
    }

    #[tokio::test]
    async fn test_mock_custom_repost_action() {
        let p = MockProvider::new();
        let target = MediaIdWire::parse("mock:track:3").unwrap();
        let res = p
            .custom_action("mock.repost", Some(&target), serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(res["reposted"], true);
        assert_eq!(res["track_id"], "mock:track:3");
        assert_eq!(res["provider"], "mock");

        // Without target
        let res_no_target = p
            .custom_action("mock.repost", None, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(res_no_target["reposted"], true);
        assert!(res_no_target["track_id"].is_null());
    }

    #[test]
    fn test_mock_capabilities_exclude_lyrics() {
        let p = MockProvider::new();
        let caps = p.capabilities();
        assert!(caps.contains(&"search".to_string()));
        assert!(caps.contains(&"playback".to_string()));
        assert!(caps.contains(&"mock.repost".to_string()));
        assert!(!caps.contains(&"lyrics.synced".to_string()));
    }
}
