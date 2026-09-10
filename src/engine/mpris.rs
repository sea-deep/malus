//! Linux Desktop MPRIS (org.mpris.MediaPlayer2) Integration via D-Bus (zbus).
//! Exposes Apple Music playback state and hardware media keys to desktop environments
//! (Waybar, Quickshell, GNOME, KDE, Hyprland, playerctl).

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tokio::sync::mpsc;
use zbus::{connection::Builder, interface, zvariant::Value};

use super::{EngineCommand, MusicKitEvent};

#[derive(Debug, Clone, Default)]
pub struct MprisState {
    pub playback_status: String, // "Playing", "Paused", "Stopped"
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: Option<String>,
    pub track_id: String,
    pub shuffle: bool,
    pub repeat: u8,
    pub duration_micros: i64,
    pub position_micros: i64,
    pub volume: f64,
}

pub struct MprisRoot;

#[interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "Malus Apple Music"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        vec![]
    }

    fn raise(&self) {}
    fn quit(&self) {}
}

pub struct MprisPlayer {
    cmd_tx: mpsc::Sender<EngineCommand>,
    state: Arc<RwLock<MprisState>>,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    async fn next(&self) {
        let _ = self.cmd_tx.send(EngineCommand::Next).await;
    }

    async fn previous(&self) {
        let _ = self.cmd_tx.send(EngineCommand::Prev).await;
    }

    async fn pause(&self) {
        let _ = self.cmd_tx.send(EngineCommand::Pause).await;
    }

    async fn play_pause(&self) {
        let _ = self.cmd_tx.send(EngineCommand::TogglePlay).await;
    }

    async fn stop(&self) {
        let _ = self.cmd_tx.send(EngineCommand::Stop).await;
    }

    async fn play(&self) {
        let _ = self.cmd_tx.send(EngineCommand::Play).await;
    }

    async fn seek(&self, offset: i64) {
        let current_micros = {
            let s = self.state.read().unwrap();
            s.position_micros
        };
        let target_secs = (current_micros.saturating_add(offset).max(0) as f64) / 1_000_000.0;
        let _ = self.cmd_tx.send(EngineCommand::Seek(target_secs)).await;
    }

    async fn set_position(&self, track_id: zbus::zvariant::ObjectPath<'_>, position: i64) {
        if track_id != track_path(&self.state.read().unwrap().track_id) {
            return;
        }
        let target_secs = (position.max(0) as f64) / 1_000_000.0;
        let _ = self.cmd_tx.send(EngineCommand::Seek(target_secs)).await;
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        let s = self.state.read().unwrap();
        if s.playback_status.is_empty() {
            "Stopped".to_string()
        } else {
            s.playback_status.clone()
        }
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        let s = self.state.read().unwrap();
        match s.repeat {
            1 => "Track",
            2 => "Playlist",
            _ => "None",
        }
        .into()
    }

    #[zbus(property)]
    async fn set_loop_status(&self, value: &str) -> zbus::fdo::Result<()> {
        use crate::model::RepeatMode;
        let mode = match value {
            "None" => RepeatMode::Off,
            "Track" => RepeatMode::One,
            "Playlist" => RepeatMode::All,
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "LoopStatus must be None, Track or Playlist".into(),
                ));
            }
        };
        self.cmd_tx
            .send(EngineCommand::SetRepeat(mode))
            .await
            .map_err(|_| zbus::fdo::Error::Failed("Audio engine disconnected".into()))
    }
    #[zbus(property)]
    async fn set_shuffle(&self, value: bool) -> zbus::fdo::Result<()> {
        self.cmd_tx
            .send(EngineCommand::SetShuffle(value))
            .await
            .map_err(|_| zbus::fdo::Error::Failed("Audio engine disconnected".into()))
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.state.read().unwrap().shuffle
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, Value<'static>> {
        let s = self.state.read().unwrap();
        let mut map = HashMap::new();
        map.insert(
            "mpris:trackid".to_string(),
            Value::from(track_path(&s.track_id)),
        );
        if !s.title.is_empty() {
            map.insert("xesam:title".to_string(), Value::from(s.title.clone()));
        }
        if !s.artist.is_empty() {
            map.insert(
                "xesam:artist".to_string(),
                Value::from(vec![s.artist.clone()]),
            );
        }
        if !s.album.is_empty() {
            map.insert("xesam:album".to_string(), Value::from(s.album.clone()));
        }
        if let Some(ref art) = s.artwork_url {
            map.insert("mpris:artUrl".to_string(), Value::from(art.clone()));
        }
        if s.duration_micros > 0 {
            map.insert("mpris:length".to_string(), Value::from(s.duration_micros));
        }
        map
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        let s = self.state.read().unwrap();
        s.volume
    }

    #[zbus(property)]
    async fn set_volume(&self, volume: f64) {
        if !volume.is_finite() {
            return;
        }
        let clamped = volume.clamp(0.0, 1.0);
        {
            let mut s = self.state.write().unwrap();
            s.volume = clamped;
        }
        let _ = self.cmd_tx.send(EngineCommand::SetVolume(clamped)).await;
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        let s = self.state.read().unwrap();
        s.position_micros
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

pub struct MprisServer {
    state: Arc<RwLock<MprisState>>,
}

fn track_path(id: &str) -> zbus::zvariant::ObjectPath<'static> {
    let hex: String = id.bytes().map(|b| format!("{b:02x}")).collect();
    zbus::zvariant::ObjectPath::try_from(format!("/org/mpris/MediaPlayer2/track/t{hex}")).unwrap()
}
impl MprisServer {
    pub async fn publish(&self, event: &MusicKitEvent, connection: &zbus::Connection) {
        self.update_state(|s| match event {
            MusicKitEvent::PlaybackState {
                is_playing,
                raw_state,
            } => {
                s.playback_status = if matches!(raw_state, Some(0 | 4 | 5 | 10)) {
                    "Stopped"
                } else if *is_playing {
                    "Playing"
                } else {
                    "Paused"
                }
                .into()
            }
            MusicKitEvent::PlaybackTime {
                current_playback_time,
                current_playback_duration,
            } => {
                s.position_micros = (*current_playback_time * 1_000_000.0) as i64;
                if let Some(d) = current_playback_duration {
                    s.duration_micros = (*d * 1_000_000.0) as i64;
                }
            }
            MusicKitEvent::NowPlaying {
                id,
                title,
                artist_name,
                album_name,
                artwork_url,
                duration,
            } => {
                s.track_id = id.clone();
                s.title = title.clone();
                s.artist = artist_name.clone();
                s.album = album_name.clone().unwrap_or_default();
                s.artwork_url = artwork_url.clone();
                s.duration_micros = (duration.unwrap_or(0.0) * 1_000_000.0) as i64;
            }
            MusicKitEvent::QueueChanged {
                volume,
                shuffle,
                repeat,
                ..
            } => {
                s.volume = *volume;
                s.shuffle = *shuffle;
                s.repeat = *repeat;
            }
            _ => {}
        });
        if let Ok(reference) = connection
            .object_server()
            .interface::<_, MprisPlayer>("/org/mpris/MediaPlayer2")
            .await
        {
            let player = reference.get().await;
            let emitter = reference.signal_emitter();
            match event {
                MusicKitEvent::PlaybackState { .. } => {
                    let _ = player.playback_status_changed(emitter).await;
                }
                MusicKitEvent::NowPlaying { .. } => {
                    let _ = player.metadata_changed(emitter).await;
                }
                MusicKitEvent::QueueChanged { .. } => {
                    let _ = player.volume_changed(emitter).await;
                    let _ = player.shuffle_changed(emitter).await;
                    let _ = player.loop_status_changed(emitter).await;
                }
                _ => {}
            }
        }
    }

    /// Spawn the MPRIS D-Bus server on the session bus.
    pub async fn start(
        cmd_tx: mpsc::Sender<EngineCommand>,
    ) -> zbus::Result<(Self, zbus::Connection)> {
        let state = Arc::new(RwLock::new(MprisState {
            playback_status: "Stopped".to_string(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            artwork_url: None,
            track_id: String::new(),
            shuffle: false,
            repeat: 0,
            duration_micros: 0,
            position_micros: 0,
            volume: 1.0,
        }));

        let root = MprisRoot;
        let player = MprisPlayer {
            cmd_tx,
            state: state.clone(),
        };

        let conn = Builder::session()?
            .name("org.mpris.MediaPlayer2.malus")?
            .serve_at("/org/mpris/MediaPlayer2", root)?
            .serve_at("/org/mpris/MediaPlayer2", player)?
            .build()
            .await?;

        Ok((Self { state }, conn))
    }

    /// Update MPRIS playback metadata from an engine event.
    pub fn update_state<F>(&self, update_fn: F)
    where
        F: FnOnce(&mut MprisState),
    {
        if let Ok(mut s) = self.state.write() {
            update_fn(&mut s);
        }
    }
}
