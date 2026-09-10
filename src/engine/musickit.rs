//! MusicKit commands use structured CDP arguments and return rejected promises.
use super::cdp::CdpClient;
use crate::model::{AudioFormat, Playlist, RadioStation, Track};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io, sync::Arc};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataRequest {
    Library,
    Catalog,
    Radio,
    Playlist(String),
    Search(String),
    Lyrics(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum MusicKitEvent {
    #[serde(rename = "authStatus")]
    AuthStatus {
        #[serde(rename = "isAuthorized")]
        is_authorized: bool,
    },
    #[serde(rename = "playbackState")]
    PlaybackState {
        #[serde(rename = "isPlaying")]
        is_playing: bool,
        #[serde(rename = "state")]
        raw_state: Option<i32>,
    },
    #[serde(rename = "playbackTime")]
    PlaybackTime {
        #[serde(rename = "currentPlaybackTime")]
        current_playback_time: f64,
        #[serde(rename = "currentPlaybackDuration")]
        current_playback_duration: Option<f64>,
    },
    #[serde(rename = "nowPlaying")]
    NowPlaying {
        id: String,
        title: String,
        #[serde(rename = "artistName")]
        artist_name: String,
        #[serde(rename = "albumName")]
        album_name: Option<String>,
        #[serde(rename = "artworkUrl")]
        artwork_url: Option<String>,
        duration: Option<f64>,
    },
    LibraryLoaded {
        tracks: Vec<Track>,
    },
    SearchResults {
        query: String,
        tracks: Vec<Track>,
    },
    CatalogLoaded {
        tracks: Vec<Track>,
    },
    PlaylistsLoaded {
        playlists: Vec<Playlist>,
    },
    PlaylistLoaded {
        id: String,
        tracks: Vec<Track>,
    },
    RadioLoaded {
        stations: Vec<RadioStation>,
    },
    LyricsLoaded {
        track_id: String,
        ttml: Option<String>,
    },
    QueueChanged {
        tracks: Vec<Track>,
        shuffle: bool,
        repeat: u8,
        volume: f64,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
    },
    Disconnected,
    RequestFailed {
        request: DataRequest,
        message: String,
    },
}

pub struct MusicKitDriver {
    cdp: Arc<CdpClient>,
}
impl MusicKitDriver {
    pub fn new(cdp: Arc<CdpClient>) -> Self {
        Self { cdp }
    }
    pub async fn initialize_bridge(&self, tx: mpsc::Sender<MusicKitEvent>) -> io::Result<()> {
        self.cdp.send_command("Page.enable", json!({})).await?;
        self.cdp.send_command("Runtime.enable", json!({})).await?;
        self.cdp.add_binding("malusDispatch").await?;
        let mut rx = self.cdp.event_rx.resubscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event)
                        if event.method == "Runtime.bindingCalled"
                            && event.params["name"] == "malusDispatch" =>
                    {
                        if let Some(raw) = event.params["payload"].as_str()
                            && let Ok(v) = serde_json::from_str::<Value>(raw)
                        {
                            let parsed = if v["event"] == "queue" {
                                Some(MusicKitEvent::QueueChanged {
                                    tracks: v["items"]
                                        .as_array()
                                        .map(|a| {
                                            a.iter().filter_map(parse_musickit_track).collect()
                                        })
                                        .unwrap_or_default(),
                                    shuffle: v["shuffle"].as_bool().unwrap_or(false),
                                    repeat: v["repeat"].as_u64().unwrap_or(0) as u8,
                                    volume: v["volume"].as_f64().unwrap_or(0.8),
                                })
                            } else {
                                serde_json::from_value(v).ok()
                            };
                            if let Some(event) = parsed
                                && tx.send(event).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        // Reinstall after Apple's redirects. Polling also handles changes made by media keys.
        let script = include_str!("bridge.js");
        self.cdp
            .send_command(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({"source":script}),
            )
            .await?;
        self.ensure_bridge().await?;
        Ok(())
    }
    pub async fn ensure_bridge(&self) -> io::Result<()> {
        self.cdp.evaluate_js(include_str!("bridge.js")).await?;
        Ok(())
    }
    pub async fn command(&self, name: &str, args: Value) -> io::Result<()> {
        self.cdp.call_function(r#"async function(name,args) {
            const mk=window.MusicKit && MusicKit.getInstance();
            if (!mk) throw new Error('Apple Music is still loading. Try again shortly.');
            const queueIndex=(target)=>{
                const index=mk.queue.position+1+target.index;
                if(mk.queue.items[index]?.id!==target.id) throw new Error('The queue changed. Please try again.');
                return index;
            };
            switch(name) {
                case 'play': await mk.play(); break;
                case 'pause': await mk.pause(); break;
                case 'stop': await mk.stop(); break;
                case 'toggle': await (mk.isPlaying ? mk.pause() : mk.play()); break;
                case 'next': await mk.skipToNextItem(); break;
                case 'previous': if(mk.currentPlaybackTime>3) await mk.seekToTime(0); else await mk.skipToPreviousItem(); break;
                case 'seek': await mk.seekToTime(args); break;
                case 'volume': mk.volume=args; break;
                case 'shuffle': mk.shuffleMode=args?1:0; break;
                case 'repeat': mk.repeatMode=args; break;
                case 'tracks': if(!args.length) return; await mk.setQueue({songs:args}); await mk.play(); break;
                case 'album': await mk.setQueue({album:args}); await mk.play(); break;
                case 'playlist': await mk.setQueue({playlist:args}); await mk.play(); break;
                case 'station': await mk.setQueue({station:args}); await mk.play(); break;
                case 'enqueue': await (args.next ? mk.playNext({song:args.id}) : mk.playLater({song:args.id})); break;
                case 'jump': await mk.changeToMediaAtIndex(queueIndex(args)); await mk.play(); break;
                case 'remove': mk.queue.splice(queueIndex(args),1); break;
                case 'clear': mk.queue.clearAfterCurrent(); break;
                case 'move': {
                    const a=queueIndex(args[0]), b=queueIndex(args[1]);
                    const item=mk.queue.items[a];
                    if(!item || b>=mk.queue.length || b<=mk.queue.position) return;
                    const start=Math.min(a,b), end=Math.max(a,b);
                    const items=mk.queue.items.slice(start,end+1);
                    items.splice(b-start,0,...items.splice(a-start,1));
                    mk.queue.splice(start,items.length,items); break;
                }
                default: throw new Error('Unknown playback command');
            }
            if(window.__malusSnapshot) window.__malusSnapshot();
        }"#, &[json!(name),args]).await?;
        Ok(())
    }
    pub async fn play(&self) -> io::Result<()> {
        self.command("play", Value::Null).await
    }
    pub async fn pause(&self) -> io::Result<()> {
        self.command("pause", Value::Null).await
    }
    pub async fn toggle_play(&self) -> io::Result<()> {
        self.command("toggle", Value::Null).await
    }
    pub async fn skip_to_next(&self) -> io::Result<()> {
        self.command("next", Value::Null).await
    }
    pub async fn skip_to_prev(&self) -> io::Result<()> {
        self.command("previous", Value::Null).await
    }
    pub async fn seek_to_time(&self, seconds: f64) -> io::Result<()> {
        if !seconds.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek time must be finite",
            ));
        }
        self.command("seek", json!(seconds.max(0.0))).await
    }
    pub async fn set_volume(&self, v: f64) -> io::Result<()> {
        if !v.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Volume must be finite",
            ));
        }
        self.command("volume", json!(v.clamp(0.0, 1.0))).await
    }
    pub async fn play_track_by_id(&self, id: &str) -> io::Result<()> {
        self.command("tracks", json!([id])).await
    }
    pub async fn api(&self, path: &str, params: Value) -> io::Result<Value> {
        self.cdp
            .call_function(
                r#"async function(path,params) {
            const mk=window.MusicKit && MusicKit.getInstance();
            if(!mk || !mk.api) throw new Error('Apple Music is still loading.');
            path=path.replace('{storefront}',mk.storefrontId || 'us');
            const result=await mk.api.music(path,params);
            return result.data;
        }"#,
                &[json!(path), params],
            )
            .await
    }
    pub async fn search_catalog(&self, term: &str, limit: usize) -> io::Result<Vec<Track>> {
        let res = self
            .api(
                "/v1/catalog/{storefront}/search",
                json!({"term":term,"types":"songs","limit":limit}),
            )
            .await?;
        Ok(parse_tracks(&res["results"]["songs"]["data"]))
    }
    pub async fn fetch_library_songs(&self, limit: usize, offset: usize) -> io::Result<Vec<Track>> {
        let res = self
            .api(
                "/v1/me/library/songs",
                json!({"limit":limit,"offset":offset}),
            )
            .await?;
        Ok(parse_tracks(&res["data"]))
    }
    pub async fn fetch_all(&self, path: &str) -> io::Result<Vec<Value>> {
        let mut values = vec![];
        let mut next = path.to_string();
        let mut seen = std::collections::HashSet::new();
        loop {
            if !seen.insert(next.clone()) {
                return Err(io::Error::other(
                    "Apple Music returned a repeated pagination link",
                ));
            }
            let params = if path.starts_with("/v1/me/library/songs") || path.ends_with("/tracks") {
                json!({"limit":100,"include":"albums,artists"})
            } else {
                json!({"limit":100})
            };
            let res = self.api(&next, params).await?;
            if let Some(items) = res["data"].as_array() {
                values.extend(items.clone());
            }
            match res["next"].as_str() {
                Some(n) if n.starts_with("/v1/") => next = n.to_string(),
                _ => break,
            }
        }
        Ok(values)
    }
    pub async fn fetch_lyrics(&self, song_id: &str) -> io::Result<Option<String>> {
        let res = self
            .cdp
            .call_function(
                r#"async function(songId) {
            const mk = window.MusicKit && MusicKit.getInstance();
            if (!mk || !mk.api) return { ttml: null, error: 'not_ready' };
            const storefront = mk.storefrontId || 'us';
            try {
                const res = await mk.api.music(`/v1/catalog/${storefront}/songs/${songId}/lyrics`);
                const ttml = res?.data?.data?.[0]?.attributes?.ttml || null;
                return { ttml: ttml };
            } catch (err) {
                return { ttml: null, error: err?.status || err?.message || 'not_found' };
            }
        }"#,
                &[json!(song_id)],
            )
            .await?;
        Ok(res["ttml"].as_str().map(|s| s.to_string()))
    }
}
pub fn parse_tracks(value: &Value) -> Vec<Track> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(parse_musickit_track).collect())
        .unwrap_or_default()
}
pub fn parse_musickit_track(value: &Value) -> Option<Track> {
    let attrs = value.get("attributes").unwrap_or(value);
    let id = value["id"]
        .as_str()
        .or_else(|| attrs["playParams"]["id"].as_str())?;
    let mut track = Track::new(
        id,
        attrs["name"]
            .as_str()
            .or_else(|| value["title"].as_str())
            .unwrap_or("Unknown title"),
        attrs["artistName"].as_str().unwrap_or("Unknown artist"),
        attrs["albumName"].as_str().unwrap_or(""),
        attrs["durationInMillis"]
            .as_f64()
            .map(|d| d / 1000.0)
            .or_else(|| value["playbackDuration"].as_f64())
            .unwrap_or(0.0) as u64,
        attrs["trackNumber"].as_u64().unwrap_or(1) as u32,
        AudioFormat::Standard,
    );
    track.catalog_id = attrs["playParams"]["catalogId"]
        .as_str()
        .map(str::to_string);
    track.artwork_url = attrs["artwork"]["url"]
        .as_str()
        .or_else(|| value["artworkURL"].as_str())
        .map(str::to_string);
    track.disc_number = attrs["discNumber"].as_u64().unwrap_or(1) as u32;
    let album = &value["relationships"]["albums"]["data"][0];
    if let Some(id) = album["id"].as_str() {
        track.album_id = id.to_string();
    }
    track.album_artist = album["attributes"]["artistName"]
        .as_str()
        .or_else(|| attrs["albumArtistName"].as_str())
        .map(str::to_string);
    let artist = &value["relationships"]["artists"]["data"][0];
    if let Some(id) = artist["id"].as_str() {
        track.artist_id = id.to_string();
    }
    track.primary_artist = artist["attributes"]["name"].as_str().map(str::to_string);
    Some(track)
}
