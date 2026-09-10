//! Browser lifetime, responsive command dispatch, and Apple Music data loading.
pub mod cdp;
pub mod discovery;
pub mod mpris;
pub mod musickit;
pub mod process;
pub mod profile;
use crate::model::{Playlist, RadioStation, RepeatMode};
pub use cdp::{CdpClient, CdpEvent};
pub use discovery::{BrowserCandidate, BrowserKind, discover_browsers, select_best_browser};
pub use musickit::{MusicKitDriver, MusicKitEvent};
pub use process::BrowserProcess;
pub use profile::ProfileManager;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueueTarget {
    pub index: usize,
    pub id: String,
}

#[derive(Debug, Clone)]
pub enum EngineCommand {
    Play,
    Pause,
    Stop,
    TogglePlay,
    Next,
    Prev,
    Seek(f64),
    SetVolume(f64),
    PlayTrack(String),
    PlayTracks(Vec<String>),
    PlayAlbum(String),
    PlayPlaylist(String),
    PlayStation(String),
    Enqueue(String, bool),
    JumpQueue(QueueTarget),
    RemoveQueue(QueueTarget),
    MoveQueue(QueueTarget, QueueTarget),
    ClearQueue,
    SetShuffle(bool),
    SetRepeat(RepeatMode),
    Search(String),
    FetchLibrary,
    FetchCatalog,
    FetchRadio,
    FetchRecommendations,
    FetchRecentlyPlayed,
    FetchPlaylist(String),
    FetchLyrics(String),
    Shutdown,
}
pub struct EngineHandle {
    pub cmd_tx: mpsc::Sender<EngineCommand>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl EngineHandle {
    pub async fn send(&self, cmd: EngineCommand) {
        let _ = self.cmd_tx.send(cmd).await;
    }
    pub async fn shutdown(mut self) {
        let _ = self.cmd_tx.try_send(EngineCommand::Shutdown);
        if let Some(mut task) = self.task.take()
            && tokio::time::timeout(Duration::from_secs(4), &mut task)
                .await
                .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

pub async fn start_engine(
    custom_browser: Option<&std::path::Path>,
    visible: bool,
) -> std::io::Result<(EngineHandle, mpsc::Receiver<MusicKitEvent>)> {
    let candidate = select_best_browser(custom_browser).ok_or_else(|| {
        std::io::Error::other("No Chromium browser found. Install Chrome or pass --browser-path.")
    })?;
    let profile = ProfileManager::new()?;
    let mut browser =
        BrowserProcess::spawn(&candidate, profile, visible, "https://music.apple.com/")?;
    let cdp = Arc::new(CdpClient::connect_to_page(browser.port, "music.apple.com").await?);
    let driver = Arc::new(MusicKitDriver::new(cdp.clone()));
    let (cmd_tx, mut cmd_rx) = mpsc::channel(64);
    let (tx, rx) = mpsc::channel(128);
    let (bridge_tx, mut bridge_rx) = mpsc::channel(128);
    driver.initialize_bridge(bridge_tx.clone()).await?;
    let mpris = mpris::MprisServer::start(cmd_tx.clone()).await.ok();
    let task = tokio::spawn(async move {
        let mut helpers = tokio::task::JoinSet::new();
        let relay_tx = tx.clone();
        helpers.spawn(async move {
            while let Some(event) = bridge_rx.recv().await {
                if let Some((server, connection)) = &mpris {
                    let _ = tokio::time::timeout(
                        Duration::from_millis(500),
                        server.publish(&event, connection),
                    )
                    .await;
                }
                if relay_tx.send(event).await.is_err() {
                    break;
                }
            }
        });
        let mut pending_command = None;
        let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
        let mut queries = tokio::task::JoinSet::new();
        let mut search_task: Option<tokio::task::AbortHandle> = None;
        let mut library_task: Option<tokio::task::AbortHandle> = None;
        loop {
            tokio::select! {
                cmd=async {if let Some(command)=pending_command.take(){Some(command)}else{cmd_rx.recv().await}}=>{
                    let Some(mut cmd)=cmd else {break;};
                    // A slider drag should land at its latest position, without replaying every cell.
                    if matches!(cmd,EngineCommand::Seek(_)|EngineCommand::SetVolume(_)) {
                        while let Ok(next)=cmd_rx.try_recv(){
                            if matches!((&cmd,&next),(EngineCommand::Seek(_),EngineCommand::Seek(_))|(EngineCommand::SetVolume(_),EngineCommand::SetVolume(_))){cmd=next;}
                            else{pending_command=Some(next);break;}
                        }
                    }
                    if matches!(cmd,EngineCommand::Shutdown){break;}
                    if matches!(cmd,EngineCommand::FetchLibrary|EngineCommand::Search(_)|EngineCommand::FetchCatalog|EngineCommand::FetchRadio|EngineCommand::FetchRecommendations|EngineCommand::FetchRecentlyPlayed|EngineCommand::FetchPlaylist(_)|EngineCommand::FetchLyrics(_)) {
                        if matches!(cmd,EngineCommand::FetchLibrary) && library_task.as_ref().is_some_and(|t|!t.is_finished()){continue;}
                        let is_search=matches!(cmd,EngineCommand::Search(_));
                        let is_library=matches!(cmd,EngineCommand::FetchLibrary);
                        if is_search&& let Some(task)=search_task.take(){task.abort();}
                        let request=match &cmd {
                            EngineCommand::Search(q)=>musickit::DataRequest::Search(q.clone()),
                            EngineCommand::FetchPlaylist(id)=>musickit::DataRequest::Playlist(id.clone()),
                            EngineCommand::FetchCatalog=>musickit::DataRequest::Catalog,
                            EngineCommand::FetchRadio=>musickit::DataRequest::Radio,
                            EngineCommand::FetchRecommendations=>musickit::DataRequest::Recommendations,
                            EngineCommand::FetchRecentlyPlayed=>musickit::DataRequest::RecentlyPlayed,
                            EngineCommand::FetchLyrics(id)=>musickit::DataRequest::Lyrics(id.clone()),
                            _=>musickit::DataRequest::Library,
                        };
                        let driver=driver.clone();let out=bridge_tx.clone();
                        let task=queries.spawn(async move {if let Err(e)=load_data(&driver,cmd,&out).await{let _=out.send(MusicKitEvent::RequestFailed{request,message:e.to_string()}).await;}});
                        if is_search{search_task=Some(task.clone());}
                        if is_library{library_task=Some(task);}
                        continue;
                    }
                    let result=match cmd {
                        EngineCommand::Play=>driver.play().await, EngineCommand::Pause=>driver.pause().await,
                        EngineCommand::Stop=>driver.command("stop",json!(null)).await,
                        EngineCommand::TogglePlay=>driver.toggle_play().await,EngineCommand::Next=>driver.skip_to_next().await,
                        EngineCommand::Prev=>driver.skip_to_prev().await,EngineCommand::Seek(s)=>driver.seek_to_time(s).await,
                        EngineCommand::SetVolume(v)=>driver.set_volume(v).await,EngineCommand::PlayTrack(id)=>driver.play_track_by_id(&id).await,
                        EngineCommand::PlayTracks(ids)=>driver.command("tracks",json!(ids)).await,
                        EngineCommand::PlayAlbum(id)=>driver.command("album",json!(id)).await,
                        EngineCommand::PlayPlaylist(id)=>driver.command("playlist",json!(id)).await,
                        EngineCommand::PlayStation(id)=>driver.command("station",json!(id)).await,
                        EngineCommand::Enqueue(id,next)=>driver.command("enqueue",json!({"id":id,"next":next})).await,
                        EngineCommand::JumpQueue(i)=>driver.command("jump",json!(i)).await,
                        EngineCommand::RemoveQueue(i)=>driver.command("remove",json!(i)).await,
                        EngineCommand::MoveQueue(a,b)=>driver.command("move",json!([a,b])).await,
                        EngineCommand::ClearQueue=>driver.command("clear",json!(null)).await,
                        EngineCommand::SetShuffle(s)=>driver.command("shuffle",json!(s)).await,
                        EngineCommand::SetRepeat(r)=>driver.command("repeat",json!(match r{RepeatMode::Off=>0,RepeatMode::One=>1,RepeatMode::All=>2})).await,
                        _=>Ok(()),
                    };
                    if let Err(e)=result{let _=tx.send(MusicKitEvent::Error{message:e.to_string()}).await;}
                }
                _=heartbeat.tick()=>{if !browser.is_alive() || cdp.ping().await.is_err(){let _=tx.send(MusicKitEvent::Disconnected).await;break;}let _=driver.ensure_bridge().await;},
                _=queries.join_next(),if !queries.is_empty()=>{},
            }
        }
        queries.abort_all();
        helpers.abort_all();
        browser.terminate();
    });
    Ok((
        EngineHandle {
            cmd_tx,
            task: Some(task),
        },
        rx,
    ))
}
async fn load_data(
    driver: &MusicKitDriver,
    cmd: EngineCommand,
    out: &mpsc::Sender<MusicKitEvent>,
) -> std::io::Result<()> {
    let event = match cmd {
        EngineCommand::Search(query) => MusicKitEvent::SearchResults {
            tracks: driver.search_catalog(&query, 25).await?,
            query,
        },
        EngineCommand::FetchLibrary => {
            let songs = driver.fetch_all("/v1/me/library/songs").await?;
            let tracks = songs
                .iter()
                .filter_map(musickit::parse_musickit_track)
                .collect();
            let _ = out.send(MusicKitEvent::LibraryLoaded { tracks }).await;
            let values = driver.fetch_all("/v1/me/library/playlists").await?;
            let playlists = values
                .iter()
                .filter_map(|v| {
                    Some(Playlist::new(
                        v["id"].as_str()?,
                        v["attributes"]["name"].as_str().unwrap_or("Playlist"),
                        "",
                        v["attributes"]["description"]["standard"]
                            .as_str()
                            .unwrap_or(""),
                        vec![],
                    ))
                })
                .collect();
            MusicKitEvent::PlaylistsLoaded { playlists }
        }
        EngineCommand::FetchCatalog => {
            let res = driver
                .api(
                    "/v1/catalog/{storefront}/charts",
                    json!({"types":"songs","limit":50}),
                )
                .await?;
            MusicKitEvent::CatalogLoaded {
                tracks: musickit::parse_tracks(&res["results"]["songs"][0]["data"]),
            }
        }
        EngineCommand::FetchRadio => {
            let res = driver
                .api(
                    "/v1/catalog/{storefront}/search",
                    json!({"term":"Apple Music","types":"stations","limit":25}),
                )
                .await?;
            let stations = res["results"]["stations"]["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| {
                            Some(RadioStation {
                                id: v["id"].as_str()?.into(),
                                name: v["attributes"]["name"].as_str().unwrap_or("Station").into(),
                                description: v["attributes"]["editorialNotes"]["short"]
                                    .as_str()
                                    .unwrap_or("Apple Music Radio")
                                    .into(),
                                tag: String::new(),
                                is_live: v["attributes"]["isLive"].as_bool().unwrap_or(false),
                                current_show: String::new(),
                                track_ids: vec![],
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            MusicKitEvent::RadioLoaded { stations }
        }
        EngineCommand::FetchRecommendations => {
            // /v1/me/recommendations returns recommendation groups.
            // Each group's relationships.contents.data[] may contain playlists.
            let res = driver
                .api("/v1/me/recommendations", json!({ "limit": 10 }))
                .await?;
            let mut mixes: Vec<crate::model::PersonalMix> = vec![];
            if let Some(groups) = res["data"].as_array() {
                for group in groups {
                    let attrs = &group["attributes"];
                    let contents = group["relationships"]["contents"]["data"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    for item in &contents {
                        // Only include playlists/personal-mixes, not songs
                        let item_type = item["type"].as_str().unwrap_or("");
                        if !item_type.contains("playlist") && item_type != "personal-mix" {
                            continue;
                        }
                        let id = item["id"].as_str().unwrap_or("").to_string();
                        if id.is_empty() {
                            continue;
                        }
                        let ia = &item["attributes"];
                        let name = ia["name"]
                            .as_str()
                            .unwrap_or_else(|| {
                                attrs["title"]["stringForDisplay"].as_str().unwrap_or("Mix")
                            })
                            .to_string();
                        // Build subtitle from curator description or editorial notes
                        let subtitle = ia["curatorName"]
                            .as_str()
                            .or_else(|| ia["description"]["short"].as_str())
                            .or_else(|| attrs["reason"]["stringForDisplay"].as_str())
                            .unwrap_or("")
                            .to_string();
                        mixes.push(crate::model::PersonalMix {
                            id,
                            name,
                            subtitle,
                            kind: item_type.to_string(),
                        });
                    }
                }
            }
            MusicKitEvent::RecommendationsLoaded { mixes }
        }
        EngineCommand::FetchRecentlyPlayed => {
            let res = driver
                .api("/v1/me/recent/played/tracks", json!({ "limit": 30 }))
                .await?;
            let tracks = musickit::parse_tracks(&res["data"]);
            MusicKitEvent::RecentlyPlayedLoaded { tracks }
        }
        EngineCommand::FetchPlaylist(id) => {
            // Encode the resource identifier as a single path segment.
            let path = url::Url::parse("https://music.apple.com/")
                .map(|mut u| {
                    u.path_segments_mut().unwrap().extend([
                        "v1",
                        "me",
                        "library",
                        "playlists",
                        &id,
                        "tracks",
                    ]);
                    u
                })
                .map_err(std::io::Error::other)?;
            let data = driver.fetch_all(path.path()).await?;
            MusicKitEvent::PlaylistLoaded {
                id,
                tracks: data
                    .iter()
                    .filter_map(musickit::parse_musickit_track)
                    .collect(),
            }
        }
        EngineCommand::FetchLyrics(track_id) => {
            let ttml = driver.fetch_lyrics(&track_id).await?;
            MusicKitEvent::LyricsLoaded { track_id, ttml }
        }
        _ => return Ok(()),
    };
    let _ = out.send(event).await;
    Ok(())
}
