use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_ipc::wire::{AuthStateWire, CatalogItemWire, LibraryKindWire, LibraryPageWire};
use malus_model::{MediaRef, PageRoute, PlaybackState, PlayerStatus, Queue, RepeatMode, Track};
use malus_service::{
    AppleCredentials, AppleError, AppleService, AppleWebSession, AuthState, OfficialAppleMusicApi,
    StaticTokenProvider, parse_apple_album, parse_apple_artist, parse_apple_artwork,
    parse_apple_playlist, parse_apple_track,
};
const AUTH: &str = "auth";
const AUTH_BROWSER: &str = "auth.browser";
const PLAYBACK: &str = "playback";
const PLAYBACK_SEEK: &str = "playback.seek";
const SEARCH: &str = "search";
const CATALOG_TRACK: &str = "catalog.track";
const CATALOG_ALBUM: &str = "catalog.album";
const CATALOG_ARTIST: &str = "catalog.artist";
const CATALOG_PLAYLIST: &str = "catalog.playlist";
const LIBRARY_TRACKS: &str = "library.tracks";
const LIBRARY_ALBUMS: &str = "library.albums";
const LIBRARY_PLAYLISTS: &str = "library.playlists";
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Mutex, mpsc},
};

async fn spawn_mock_apple_api(
    responses: Vec<(u16, serde_json::Value)>,
) -> (String, tokio::sync::oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
    let responses = Arc::new(tokio::sync::Mutex::new(responses));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                accept_res = listener.accept() => {
                    let (mut stream, _) = match accept_res {
                        Ok(p) => p,
                        Err(_) => break,
                    };
                    let resps = responses.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 4096];
                        let _ = stream.read(&mut buf).await;
                        let (status, val) = {
                            let mut g = resps.lock().await;
                            if !g.is_empty() { g.remove(0) } else { (200, serde_json::json!({})) }
                        };
                        let body = val.to_string();
                        let resp_str = format!(
                            "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            status, body.len(), body
                        );
                        let _ = stream.write_all(resp_str.as_bytes()).await;
                    });
                }
            }
        }
    });

    (format!("http://{addr}"), tx)
}

struct MockAppleWebSession {
    probe_result: Mutex<Result<AuthState, AppleError>>,
    begin_result: Mutex<Result<AuthState, AppleError>>,
    logout_called: Mutex<bool>,
    last_played_track: Mutex<Option<String>>,
    last_played_kind: Mutex<Option<String>>,
    is_paused: Mutex<bool>,
    is_stopped: Mutex<bool>,
    last_seek_ms: Mutex<Option<u64>>,
    volume: Mutex<u8>,
    shuffle: Mutex<bool>,
    repeat: Mutex<RepeatMode>,
    last_play_next: Mutex<Option<(String, String)>>,
    last_play_later: Mutex<Option<(String, String)>>,
    last_jump_idx: Mutex<Option<usize>>,
    last_remove_idx: Mutex<Option<usize>>,
    last_move: Mutex<Option<(usize, usize)>>,
    clear_upcoming_called: Mutex<bool>,
    skip_next_called: Mutex<bool>,
    skip_prev_called: Mutex<bool>,
    event_sink: Mutex<Option<mpsc::UnboundedSender<malus_service::PlaybackEvent>>>,
}

impl MockAppleWebSession {
    fn new() -> Self {
        Self {
            probe_result: Mutex::new(Ok(AuthState::NeedsAuth)),
            begin_result: Mutex::new(Ok(AuthState::Authenticated)),
            logout_called: Mutex::new(false),
            last_played_track: Mutex::new(None),
            last_played_kind: Mutex::new(None),
            is_paused: Mutex::new(false),
            is_stopped: Mutex::new(true),
            last_seek_ms: Mutex::new(None),
            volume: Mutex::new(100),
            shuffle: Mutex::new(false),
            repeat: Mutex::new(RepeatMode::Off),
            last_play_next: Mutex::new(None),
            last_play_later: Mutex::new(None),
            last_jump_idx: Mutex::new(None),
            last_remove_idx: Mutex::new(None),
            last_move: Mutex::new(None),
            clear_upcoming_called: Mutex::new(false),
            skip_next_called: Mutex::new(false),
            skip_prev_called: Mutex::new(false),
            event_sink: Mutex::new(None),
        }
    }
}

#[async_trait]
impl AppleWebSession for MockAppleWebSession {
    async fn probe_auth(&self) -> Result<AuthState, AppleError> {
        let guard = self.probe_result.lock().await;
        match &*guard {
            Ok(s) => Ok(*s),
            Err(AppleError::ProfileBusy) => Err(AppleError::ProfileBusy),
            Err(e) => Err(AppleError::Internal(e.to_string())),
        }
    }

    async fn begin_auth(&self, _timeout: Duration) -> Result<AuthState, AppleError> {
        let guard = self.begin_result.lock().await;
        match &*guard {
            Ok(s) => Ok(*s),
            Err(AppleError::AuthCancelled) => Err(AppleError::AuthCancelled),
            Err(AppleError::AuthTimeout) => Err(AppleError::AuthTimeout),
            Err(AppleError::ProfileBusy) => Err(AppleError::ProfileBusy),
            Err(e) => Err(AppleError::Internal(e.to_string())),
        }
    }

    async fn refresh_tokens(&self) -> Result<AppleCredentials, AppleError> {
        Ok(AppleCredentials::new("dev_token", "user_token", "us"))
    }

    async fn logout(&self) -> Result<(), AppleError> {
        *self.logout_called.lock().await = true;
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), AppleError> {
        Ok(())
    }

    async fn set_queue(&self, kind: &str, id: &str) -> Result<(), AppleError> {
        *self.last_played_track.lock().await = Some(id.to_string());
        *self.last_played_kind.lock().await = Some(kind.to_string());
        *self.is_stopped.lock().await = false;
        *self.is_paused.lock().await = false;

        let guard = self.event_sink.lock().await;
        if let Some(ref sink) = *guard {
            let mref = match kind {
                "album" => MediaRef::Album(id.to_string()),
                "playlist" => MediaRef::Playlist(id.to_string()),
                "station" => MediaRef::Station(id.to_string()),
                _ => MediaRef::Song(id.to_string()),
            };
            let _ = sink.send(malus_service::PlaybackEvent::Status(PlayerStatus {
                state: PlaybackState::Playing,
                current_track: Some(Track::new(mref, "Mock Apple Song", "Mock Artist")),
                position_ms: 0,
                duration_ms: 180_000,
                volume: 100,
                muted: false,
                shuffle: false,
                repeat: RepeatMode::Off,
            }));
        }
        Ok(())
    }

    async fn set_queue_at_index(
        &self,
        kind: &str,
        id: &str,
        _start_index: usize,
    ) -> Result<(), AppleError> {
        self.set_queue(kind, id).await
    }

    async fn set_queue_with_shuffle(
        &self,
        kind: &str,
        id: &str,
        shuffle: bool,
    ) -> Result<(), AppleError> {
        self.set_shuffle(shuffle).await?;
        self.set_queue(kind, id).await
    }

    async fn restart_current_item(&self) -> Result<(), AppleError> {
        self.seek(0).await?;
        self.resume().await
    }

    async fn pause(&self) -> Result<(), AppleError> {
        *self.is_paused.lock().await = true;
        Ok(())
    }

    async fn resume(&self) -> Result<(), AppleError> {
        *self.is_paused.lock().await = false;
        *self.is_stopped.lock().await = false;
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppleError> {
        *self.is_stopped.lock().await = true;
        Ok(())
    }

    async fn seek(&self, position_ms: u64) -> Result<(), AppleError> {
        *self.last_seek_ms.lock().await = Some(position_ms);
        Ok(())
    }

    async fn set_volume(&self, volume: u8) -> Result<(), AppleError> {
        *self.volume.lock().await = volume;
        Ok(())
    }
    async fn set_shuffle(&self, shuffle: bool) -> Result<(), AppleError> {
        *self.shuffle.lock().await = shuffle;
        Ok(())
    }
    async fn set_repeat(&self, repeat: RepeatMode) -> Result<(), AppleError> {
        *self.repeat.lock().await = repeat;
        Ok(())
    }

    async fn get_status(&self) -> Result<PlayerStatus, AppleError> {
        let paused = *self.is_paused.lock().await;
        let stopped = *self.is_stopped.lock().await;
        let track_id = self.last_played_track.lock().await.clone();

        let state = if stopped {
            PlaybackState::Stopped
        } else if paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        };

        let current_track =
            track_id.map(|id| Track::new(MediaRef::Song(id), "Mock Apple Song", "Mock Artist"));

        Ok(PlayerStatus {
            state,
            current_track,
            position_ms: self.last_seek_ms.lock().await.unwrap_or(0),
            duration_ms: 180_000,
            volume: *self.volume.lock().await,
            muted: false,
            shuffle: *self.shuffle.lock().await,
            repeat: *self.repeat.lock().await,
        })
    }

    async fn get_queue(&self) -> Result<Queue, AppleError> {
        let track_id = self.last_played_track.lock().await.clone();
        match track_id {
            Some(id) => {
                let track = Track::new(MediaRef::Song(id), "Mock Apple Song", "Mock Artist");
                Ok(Queue::with_items(vec![track], Some(0)))
            }
            None => Ok(Queue::new()),
        }
    }

    async fn skip_to_next(&self) -> Result<(), AppleError> {
        *self.skip_next_called.lock().await = true;
        Ok(())
    }

    async fn skip_to_previous(&self) -> Result<(), AppleError> {
        *self.skip_prev_called.lock().await = true;
        Ok(())
    }

    async fn play_next(&self, kind: &str, id: &str) -> Result<(), AppleError> {
        *self.last_play_next.lock().await = Some((kind.to_string(), id.to_string()));
        Ok(())
    }

    async fn play_later(&self, kind: &str, id: &str) -> Result<(), AppleError> {
        *self.last_play_later.lock().await = Some((kind.to_string(), id.to_string()));
        Ok(())
    }

    async fn queue_jump(&self, index: usize) -> Result<(), AppleError> {
        *self.last_jump_idx.lock().await = Some(index);
        Ok(())
    }

    async fn queue_remove(&self, index: usize) -> Result<(), AppleError> {
        *self.last_remove_idx.lock().await = Some(index);
        Ok(())
    }

    async fn queue_move(&self, from: usize, to: usize) -> Result<(), AppleError> {
        *self.last_move.lock().await = Some((from, to));
        Ok(())
    }

    async fn queue_clear_upcoming(&self) -> Result<(), AppleError> {
        *self.clear_upcoming_called.lock().await = true;
        Ok(())
    }

    fn set_event_sink(&self, sink: mpsc::UnboundedSender<malus_service::PlaybackEvent>) {
        if let Ok(mut guard) = self.event_sink.try_lock() {
            *guard = Some(sink);
        }
    }
}

#[tokio::test]
async fn test_provider_identity_and_capabilities() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock);

    assert_eq!(service.id(), "apple");
    assert_eq!(service.name(), "Apple Music");
    assert_eq!(
        service.capabilities(),
        vec![
            AUTH.to_string(),
            AUTH_BROWSER.to_string(),
            PLAYBACK.to_string(),
            PLAYBACK_SEEK.to_string(),
            SEARCH.to_string(),
            CATALOG_TRACK.to_string(),
            CATALOG_ALBUM.to_string(),
            CATALOG_ARTIST.to_string(),
            CATALOG_PLAYLIST.to_string(),
            LIBRARY_TRACKS.to_string(),
            LIBRARY_ALBUMS.to_string(),
            LIBRARY_PLAYLISTS.to_string(),
        ]
    );
}

#[tokio::test]
async fn test_auth_status_probing() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.probe_result.lock().await = Ok(AuthState::NeedsAuth);

    let service = AppleService::with_session(mock.clone());
    let status = service.auth_status().await.expect("probe auth status");

    assert_eq!(status.state, AuthStateWire::NeedsAuth);

    // Simulate returning Authenticated on subsequent probe
    *mock.probe_result.lock().await = Ok(AuthState::Authenticated);
    let status = service.auth_status().await.expect("probe auth status");
    assert_eq!(status.state, AuthStateWire::Authenticated);
}

#[tokio::test]
async fn test_auth_begin_success() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Ok(AuthState::Authenticated);

    let service = AppleService::with_session(mock);
    let status = service.auth_begin().await.expect("auth begin");

    assert_eq!(status.state, AuthStateWire::Authenticated);
}

#[tokio::test]
async fn test_auth_begin_cancellation() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::AuthCancelled);

    let service = AppleService::with_session(mock);
    let err = service.auth_begin().await.expect_err("should cancel");

    assert!(err.to_string().contains("closed by user"));
}

#[tokio::test]
async fn test_auth_begin_timeout() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::AuthTimeout);

    let service = AppleService::with_session(mock);
    let err = service.auth_begin().await.expect_err("should time out");

    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn test_auth_begin_profile_busy() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::ProfileBusy);

    let service = AppleService::with_session(mock);
    let err = service.auth_begin().await.expect_err("should fail if busy");

    assert!(err.to_string().contains("already in use"));
}

#[tokio::test]
async fn test_auth_logout() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());

    let status = service.auth_logout().await.expect("logout");
    assert_eq!(status.state, AuthStateWire::NeedsAuth);
    assert!(*mock.logout_called.lock().await);
}

#[tokio::test]
async fn test_playback_play_valid_media_id() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());

    service
        .play(&MediaRef::Song("1440857781".to_string()))
        .await
        .expect("play track");
    assert_eq!(
        mock.last_played_track.lock().await.as_deref(),
        Some("1440857781")
    );

    let status = service.get_status().await.expect("get status");
    assert_eq!(status.state, PlaybackState::Playing);
    assert_eq!(
        status.current_track.as_ref().map(|t| &t.id),
        Some(&MediaRef::Song("1440857781".to_string()))
    );
}

#[tokio::test]
async fn test_playback_play_album_kind() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock);

    // Albums are now accepted via set_queue
    service
        .play(&MediaRef::Album("12345".to_string()))
        .await
        .expect("album play should succeed");
}

#[tokio::test]
async fn test_playback_controls_forwarding() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());

    service
        .play(&MediaRef::Song("123".to_string()))
        .await
        .unwrap();
    assert!(!*mock.is_paused.lock().await);

    service.pause().await.unwrap();
    assert!(*mock.is_paused.lock().await);

    service.resume().await.unwrap();
    assert!(!*mock.is_paused.lock().await);

    service.seek(45_000).await.unwrap();
    assert_eq!(*mock.last_seek_ms.lock().await, Some(45_000));

    service.stop().await.unwrap();
    assert!(*mock.is_stopped.lock().await);
}

#[tokio::test]
async fn test_playback_all_media_kinds() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());

    // 1. Song
    service
        .play(&MediaRef::Song("123".to_string()))
        .await
        .unwrap();
    assert_eq!(mock.last_played_kind.lock().await.as_deref(), Some("song"));
    assert_eq!(mock.last_played_track.lock().await.as_deref(), Some("123"));

    // 2. Album
    service
        .play(&MediaRef::Album("456".to_string()))
        .await
        .unwrap();
    assert_eq!(mock.last_played_kind.lock().await.as_deref(), Some("album"));
    assert_eq!(mock.last_played_track.lock().await.as_deref(), Some("456"));

    // 3. Playlist
    service
        .play(&MediaRef::Playlist("pl.789".to_string()))
        .await
        .unwrap();
    assert_eq!(
        mock.last_played_kind.lock().await.as_deref(),
        Some("playlist")
    );
    assert_eq!(
        mock.last_played_track.lock().await.as_deref(),
        Some("pl.789")
    );

    // 4. Station
    service
        .play(&MediaRef::Station("ra.101".to_string()))
        .await
        .unwrap();
    assert_eq!(
        mock.last_played_kind.lock().await.as_deref(),
        Some("station")
    );
    assert_eq!(
        mock.last_played_track.lock().await.as_deref(),
        Some("ra.101")
    );
}

#[tokio::test]
async fn test_queue_mutations_forwarding() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());

    // Play next
    service
        .play_next(&MediaRef::Song("111".to_string()))
        .await
        .unwrap();
    assert_eq!(
        mock.last_play_next.lock().await.clone(),
        Some(("song".to_string(), "111".to_string()))
    );

    // Play later
    service
        .play_later(&MediaRef::Album("222".to_string()))
        .await
        .unwrap();
    assert_eq!(
        mock.last_play_later.lock().await.clone(),
        Some(("album".to_string(), "222".to_string()))
    );

    // Jump
    service.queue_jump(5).await.unwrap();
    assert_eq!(*mock.last_jump_idx.lock().await, Some(5));

    // Remove
    service.queue_remove(3).await.unwrap();
    assert_eq!(*mock.last_remove_idx.lock().await, Some(3));

    // Move
    service.queue_move(1, 4).await.unwrap();
    assert_eq!(*mock.last_move.lock().await, Some((1, 4)));

    // Clear upcoming
    service.queue_clear_upcoming().await.unwrap();
    assert!(*mock.clear_upcoming_called.lock().await);

    // Skip next
    service.skip_to_next().await.unwrap();
    assert!(*mock.skip_next_called.lock().await);

    // Skip previous
    service.skip_to_previous().await.unwrap();
    assert!(*mock.skip_prev_called.lock().await);
}

#[tokio::test]
async fn test_apple_search_and_catalog() {
    let mock = Arc::new(MockAppleWebSession::new());

    let (url, _shutdown) = spawn_mock_apple_api(vec![
        // 1. Search
        (200, serde_json::json!({
            "results": {
                "songs": {
                    "data": [
                        { "id": "123", "type": "songs", "attributes": { "name": "Daft Punk Song", "artistName": "Test Artist" } }
                    ]
                },
                "albums": {
                    "data": [
                        { "id": "456", "type": "albums", "attributes": { "name": "Daft Punk Album", "trackCount": 10 } }
                    ]
                }
            }
        })),
        // 2. Track lookup
        (200, serde_json::json!({
            "data": [
                { "id": "123", "type": "songs", "attributes": { "name": "Mock Track", "artistName": "Mock Artist" } }
            ]
        })),
        // 3. Album lookup
        (200, serde_json::json!({
            "data": [
                { "id": "456", "type": "albums", "attributes": { "name": "Mock Album", "trackCount": 12 } }
            ]
        })),
        // 4. Artist lookup
        (200, serde_json::json!({
            "data": [
                { "id": "789", "type": "artists", "attributes": { "name": "Mock Artist" } }
            ]
        })),
        // 5. Playlist lookup
        (200, serde_json::json!({
            "data": [
                { "id": "pl.abc", "type": "playlists", "attributes": { "name": "Mock Playlist", "curatorName": "Apple Music", "trackCount": 50 } }
            ]
        })),
    ]).await;

    let creds = AppleCredentials::new("dev", "user", "us");
    let token_provider = Arc::new(StaticTokenProvider::new(creds));
    let api = Arc::new(OfficialAppleMusicApi::with_base_url(token_provider, url));
    let service = AppleService::with_session_and_api(mock.clone(), api);

    // 1. Search
    let search_res = service
        .search("Daft Punk", &[], 10, None)
        .await
        .expect("search");
    let tracks = search_res.tracks.expect("tracks");
    assert_eq!(tracks.items.len(), 1);
    assert_eq!(tracks.items[0].id, MediaRef::Song("123".to_string()));
    assert_eq!(tracks.items[0].title, "Daft Punk Song");

    let albums = search_res.albums.expect("albums");
    assert_eq!(albums.items.len(), 1);
    assert_eq!(albums.items[0].id, MediaRef::Album("456".to_string()));

    // 2. Get Catalog Item (track)
    let item = service
        .get_catalog_item(&MediaRef::Song("123".to_string()))
        .await
        .expect("track item");
    match item {
        CatalogItemWire::Track(t) => {
            assert_eq!(t.id, MediaRef::Song("123".to_string()));
            assert_eq!(t.title, "Mock Track");
        }
        _ => panic!("Expected Track"),
    }

    // 3. Get Catalog Item (album)
    let item = service
        .get_catalog_item(&MediaRef::Album("456".to_string()))
        .await
        .expect("album item");
    match item {
        CatalogItemWire::Album(a) => {
            assert_eq!(a.id, MediaRef::Album("456".to_string()));
            assert_eq!(a.title, "Mock Album");
            assert_eq!(a.track_count, Some(12));
        }
        _ => panic!("Expected Album"),
    }

    // 4. Get Catalog Item (artist)
    let item = service
        .get_catalog_item(&MediaRef::Artist("789".to_string()))
        .await
        .expect("artist item");
    match item {
        CatalogItemWire::Artist(a) => {
            assert_eq!(a.id, MediaRef::Artist("789".to_string()));
            assert_eq!(a.name, "Mock Artist");
        }
        _ => panic!("Expected Artist"),
    }

    // 5. Get Catalog Item (playlist)
    let item = service
        .get_catalog_item(&MediaRef::Playlist("pl.abc".to_string()))
        .await
        .expect("playlist item");
    match item {
        CatalogItemWire::Playlist(p) => {
            assert_eq!(p.id, MediaRef::Playlist("pl.abc".to_string()));
            assert_eq!(p.title, "Mock Playlist");
        }
        _ => panic!("Expected Playlist"),
    }
}

#[tokio::test]
async fn test_apple_collection_and_library() {
    let mock = Arc::new(MockAppleWebSession::new());

    let (url, _shutdown) = spawn_mock_apple_api(vec![
        // 1. Collection items
        (200, serde_json::json!({
            "data": [
                { "id": "101", "type": "songs", "attributes": { "name": "Collection Song 1", "artistName": "Collection Artist" } },
                { "id": "102", "type": "songs", "attributes": { "name": "Collection Song 2", "artistName": "Collection Artist" } }
            ]
        })),
        // 2. Library tracks
        (200, serde_json::json!({
            "data": [
                { "id": "i.123", "type": "library-songs", "attributes": { "name": "Library Song", "artistName": "Library Artist" } }
            ]
        })),
        // 3. Library albums
        (200, serde_json::json!({
            "data": [
                { "id": "l.456", "type": "library-albums", "attributes": { "name": "Library Album", "trackCount": 8 } }
            ]
        })),
        // 4. Library playlists
        (200, serde_json::json!({
            "data": [
                { "id": "p.789", "type": "library-playlists", "attributes": { "name": "Library Playlist", "trackCount": 20 } }
            ]
        })),
    ]).await;

    let creds = AppleCredentials::new("dev", "user", "us");
    let token_provider = Arc::new(StaticTokenProvider::new(creds));
    let api = Arc::new(OfficialAppleMusicApi::with_base_url(token_provider, url));
    let service = AppleService::with_session_and_api(mock.clone(), api);

    // 1. Collection items
    let page = service
        .get_collection_items(&MediaRef::Album("456".to_string()), 20, None)
        .await
        .expect("collection items");
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, MediaRef::Song("101".to_string()));
    assert_eq!(page.items[1].id, MediaRef::Song("102".to_string()));

    // 2. Library tracks
    let lib_tracks = service
        .get_library(LibraryKindWire::Tracks, 20, None)
        .await
        .expect("library tracks");
    match lib_tracks {
        LibraryPageWire::Tracks(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, MediaRef::Song("i.123".to_string()));
        }
        _ => panic!("Expected LibraryPageWire::Tracks"),
    }

    // 3. Library albums
    let lib_albums = service
        .get_library(LibraryKindWire::Albums, 20, None)
        .await
        .expect("library albums");
    match lib_albums {
        LibraryPageWire::Albums(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, MediaRef::Album("l.456".to_string()));
        }
        _ => panic!("Expected LibraryPageWire::Albums"),
    }

    // 4. Library playlists
    let lib_playlists = service
        .get_library(LibraryKindWire::Playlists, 20, None)
        .await
        .expect("library playlists");
    match lib_playlists {
        LibraryPageWire::Playlists(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, MediaRef::Playlist("p.789".to_string()));
        }
        _ => panic!("Expected LibraryPageWire::Playlists"),
    }
}

#[test]
fn test_apple_json_normalization_fixtures() {
    let track_json = serde_json::json!({
        "id": "1440857781",
        "type": "songs",
        "attributes": {
            "name": "Get Lucky",
            "artistName": "Daft Punk feat. Pharrell Williams",
            "albumName": "Random Access Memories",
            "durationInMillis": 369626,
            "trackNumber": 8,
            "discNumber": 1,
            "artwork": {
                "url": "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/{w}x{h}bb.jpg",
                "width": 1400,
                "height": 1400
            },
            "url": "https://music.apple.com/us/album/get-lucky/1440857780?i=1440857781"
        },
        "relationships": {
            "artists": {
                "data": [
                    { "id": "5468295", "attributes": { "name": "Daft Punk" } }
                ]
            },
            "albums": {
                "data": [
                    { "id": "1440857780", "attributes": { "name": "Random Access Memories" } }
                ]
            }
        }
    });

    let track = parse_apple_track(&track_json).expect("parse track");
    assert_eq!(track.id, MediaRef::Song("1440857781".to_string()));
    assert_eq!(track.title, "Get Lucky");
    assert_eq!(track.artists.len(), 1);
    assert_eq!(
        track.artists[0].id,
        Some(MediaRef::Artist("5468295".to_string()))
    );
    assert_eq!(track.artists[0].name, "Daft Punk");
    assert_eq!(
        track.album.as_ref().and_then(|a| a.id.clone()),
        Some(MediaRef::Album("1440857780".to_string()))
    );
    assert_eq!(
        track.album.as_ref().map(|a| a.title.as_str()),
        Some("Random Access Memories")
    );
    assert_eq!(track.duration_ms, Some(369626));
    assert_eq!(track.track_number, Some(8));
    assert_eq!(track.disc_number, Some(1));
    assert!(track.artwork.is_some());

    let album_json = serde_json::json!({
        "id": "1440857780",
        "type": "albums",
        "attributes": {
            "name": "Random Access Memories",
            "artistName": "Daft Punk",
            "trackCount": 13,
            "releaseDate": "2013-05-17",
            "artwork": { "url": "https://example.com/art.jpg", "width": 1000, "height": 1000 },
            "url": "https://music.apple.com/us/album/random-access-memories/1440857780"
        }
    });

    let album = parse_apple_album(&album_json).expect("parse album");
    assert_eq!(album.id, MediaRef::Album("1440857780".to_string()));
    assert_eq!(album.title, "Random Access Memories");
    assert_eq!(album.track_count, Some(13));
    assert_eq!(album.release_date.as_deref(), Some("2013-05-17"));

    let artist_json = serde_json::json!({
        "id": "5468295",
        "type": "artists",
        "attributes": {
            "name": "Daft Punk",
            "genreNames": ["Electronic", "Dance"]
        }
    });

    let artist = parse_apple_artist(&artist_json).expect("parse artist");
    assert_eq!(artist.id, MediaRef::Artist("5468295".to_string()));
    assert_eq!(artist.name, "Daft Punk");

    let playlist_json = serde_json::json!({
        "id": "pl.u-xyz",
        "type": "playlists",
        "attributes": {
            "name": "Summer Vibes",
            "curatorName": "Apple Music Electronic",
            "trackCount": 42
        }
    });

    let playlist = parse_apple_playlist(&playlist_json).expect("parse playlist");
    assert_eq!(playlist.id, MediaRef::Playlist("pl.u-xyz".to_string()));
    assert_eq!(playlist.title, "Summer Vibes");
    assert_eq!(playlist.curator.as_deref(), Some("Apple Music Electronic"));
    assert_eq!(playlist.track_count, Some(42));
}

#[test]
fn test_apple_artwork_template_normalization() {
    let raw_art = serde_json::json!({
        "url": "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/e8/43/5f/e8435ffa-b6b9-b171-40ab-4ff3959ab661/886443919266.jpg/{w}x{h}bb.jpg",
        "width": 3000,
        "height": 3000
    });

    let art = parse_apple_artwork(&raw_art).expect("parsed artwork");
    assert_eq!(
        art.url,
        "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/e8/43/5f/e8435ffa-b6b9-b171-40ab-4ff3959ab661/886443919266.jpg/600x600bb.jpg"
    );
    assert!(!art.url.contains("{w}"));
    assert!(!art.url.contains("{h}"));
    assert_eq!(art.width, Some(3000));
    assert_eq!(art.height, Some(3000));
}

#[tokio::test]
async fn test_apple_pages_navigation_manifest() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock);

    let nav = service.get_navigation();
    assert_eq!(nav.default_route, PageRoute::Home);
    assert_eq!(nav.groups.len(), 3);

    // Discover group
    assert_eq!(nav.groups[0].id, "discover");
    assert_eq!(nav.groups[0].entries.len(), 3);
    assert_eq!(nav.groups[0].entries[0].route, PageRoute::Home);
    assert_eq!(nav.groups[0].entries[1].route, PageRoute::New);
    assert_eq!(nav.groups[0].entries[2].route, PageRoute::Radio);

    // Library group
    assert_eq!(nav.groups[1].id, "library");
    assert_eq!(nav.groups[1].entries.len(), 5);
    assert_eq!(
        nav.groups[1].entries[0].route,
        PageRoute::LibraryRecentlyAdded
    );
    assert_eq!(nav.groups[1].entries[1].route, PageRoute::LibraryArtists);
    assert_eq!(nav.groups[1].entries[2].route, PageRoute::LibraryAlbums);
    assert_eq!(nav.groups[1].entries[3].route, PageRoute::LibrarySongs);
    assert_eq!(nav.groups[1].entries[4].route, PageRoute::LibraryMadeForYou);

    // Playlists group
    assert_eq!(nav.groups[2].id, "playlists");
    assert_eq!(nav.groups[2].entries[0].route, PageRoute::LibraryPlaylists);
    assert_eq!(
        nav.groups[2].entries[1].route,
        PageRoute::Playlist("p.VRU64LvNXP".to_string())
    );
}

#[tokio::test]
async fn test_apple_pages_home_generation() {
    let mock = Arc::new(MockAppleWebSession::new());
    let (url, _shutdown) = spawn_mock_apple_api(vec![
        // 1. Recommendations
        (
            200,
            serde_json::json!({
                "data": [
                    {
                        "id": "rec.1",
                        "type": "personal-recommendation",
                        "attributes": {
                            "title": { "stringForDisplay": "Favorites Mix" }
                        },
                        "relationships": {
                            "contents": {
                                "data": [
                                    {
                                        "id": "pl.fav",
                                        "type": "playlists",
                                        "attributes": {
                                            "name": "Favorites Mix",
                                            "curatorName": "Apple Music",
                                            "artwork": { "url": "https://example.com/{w}x{h}bb.jpg" }
                                        }
                                    }
                                ]
                            }
                        }
                    }
                ],
                "next": "/v1/me/recommendations?offset=10"
            }),
        ),
        // 2. Recently Played
        (
            200,
            serde_json::json!({
                "data": [
                    {
                        "id": "1440857781",
                        "type": "songs",
                        "attributes": {
                            "name": "Get Lucky",
                            "artistName": "Daft Punk",
                            "albumName": "Random Access Memories",
                            "durationInMillis": 369626
                        }
                    }
                ]
            }),
        ),
        // 3. Heavy Rotation
        (
            200,
            serde_json::json!({
                "data": [
                    {
                        "id": "1440857780",
                        "type": "albums",
                        "attributes": {
                            "name": "Random Access Memories",
                            "artistName": "Daft Punk"
                        }
                    }
                ]
            }),
        ),
    ])
    .await;

    let creds = AppleCredentials::new("dev", "user", "us");
    let token_provider = Arc::new(StaticTokenProvider::new(creds));
    let api = Arc::new(OfficialAppleMusicApi::with_base_url(token_provider, url));
    let service = AppleService::with_session_and_api(mock, api);

    let page = service.get_page(&PageRoute::Home).await.expect("home page");
    assert_eq!(page.id, "home");
    assert_eq!(page.title, "Home");
    assert_eq!(page.sections.len(), 4);

    // Section 0: Top Picks for You
    assert_eq!(page.sections[0].id, "top-picks");
    assert_eq!(page.sections[0].title.as_deref(), Some("Top Picks for You"));
    assert_eq!(page.sections[0].items.len(), 1);
    assert_eq!(page.sections[0].items[0].id, "pl.fav");

    // Section 1: Recently Played
    assert_eq!(page.sections[1].id, "recently-played");
    assert_eq!(page.sections[1].items.len(), 1);
    assert_eq!(page.sections[1].items[0].id, "1440857781");

    // Section 2: Recommendations
    assert_eq!(page.sections[2].title.as_deref(), Some("Favorites Mix"));
    assert_eq!(page.sections[2].items.len(), 1);
    assert_eq!(page.sections[2].items[0].id, "pl.fav");
    assert_eq!(
        page.sections[2].items[0].entity,
        Some(MediaRef::Playlist("pl.fav".to_string()))
    );

    // Section 3: Heavy Rotation
    assert_eq!(page.sections[3].id, "heavy-rotation");
    assert_eq!(page.sections[3].items.len(), 1);
    assert_eq!(page.sections[3].items[0].id, "1440857780");
    assert_eq!(
        page.sections[3].items[0].entity,
        Some(MediaRef::Album("1440857780".to_string()))
    );

    // Page continuation
    assert!(page.continuation.is_some());
}

#[tokio::test]
async fn test_apple_pages_detail_generation() {
    let mock = Arc::new(MockAppleWebSession::new());
    let (url, _shutdown) = spawn_mock_apple_api(vec![
        // 1. Album detail: GET /v1/catalog/{storefront}/albums/1440857780?include=tracks
        (
            200,
            serde_json::json!({
                "data": [
                    {
                        "id": "1440857780",
                        "type": "albums",
                        "attributes": {
                            "name": "Random Access Memories",
                            "artistName": "Daft Punk",
                            "releaseDate": "2013-05-17",
                            "genreNames": ["Electronic"]
                        },
                        "relationships": {
                            "tracks": {
                                "data": [
                                    {
                                        "id": "1440857781",
                                        "type": "songs",
                                        "attributes": {
                                            "name": "Get Lucky",
                                            "artistName": "Daft Punk",
                                            "trackNumber": 1,
                                            "durationInMillis": 369626
                                        }
                                    }
                                ]
                            }
                        }
                    }
                ]
            }),
        ),
        // 2. Playlist detail: GET /v1/catalog/{storefront}/playlists/pl.123?include=tracks
        (
            200,
            serde_json::json!({
                "data": [
                    {
                        "id": "pl.123",
                        "type": "playlists",
                        "attributes": {
                            "name": "Dance Party",
                            "curatorName": "Apple Music Dance"
                        },
                        "relationships": {
                            "tracks": {
                                "data": [
                                    {
                                        "id": "1440857781",
                                        "type": "songs",
                                        "attributes": {
                                            "name": "Get Lucky",
                                            "artistName": "Daft Punk",
                                            "durationInMillis": 369626
                                        }
                                    }
                                ]
                            }
                        }
                    }
                ]
            }),
        ),
    ])
    .await;

    let creds = AppleCredentials::new("dev", "user", "us");
    let token_provider = Arc::new(StaticTokenProvider::new(creds));
    let api = Arc::new(OfficialAppleMusicApi::with_base_url(token_provider, url));
    let service = AppleService::with_session_and_api(mock, api);

    // Album detail
    let album_page = service
        .get_page(&PageRoute::Album("1440857780".to_string()))
        .await
        .expect("album page");
    assert_eq!(album_page.id, "album:1440857780");
    assert_eq!(album_page.title, "Random Access Memories");
    assert!(album_page.header.is_some());
    assert_eq!(album_page.sections.len(), 1);
    assert_eq!(album_page.sections[0].id, "tracks");
    assert_eq!(album_page.sections[0].items[0].id, "1440857781");
    assert_eq!(
        album_page.sections[0].items[0].entity,
        Some(MediaRef::Song("1440857781".to_string()))
    );

    // Playlist detail
    let playlist_page = service
        .get_page(&PageRoute::Playlist("pl.123".to_string()))
        .await
        .expect("playlist page");
    assert_eq!(playlist_page.id, "playlist:pl.123");
    assert_eq!(playlist_page.title, "Dance Party");
    assert!(playlist_page.header.is_some());
    assert_eq!(playlist_page.sections.len(), 1);
    assert_eq!(playlist_page.sections[0].id, "tracks");
    assert_eq!(playlist_page.sections[0].items[0].id, "1440857781");
    assert_eq!(
        playlist_page.sections[0].items[0].entity,
        Some(MediaRef::Song("1440857781".to_string()))
    );
}

#[tokio::test]
async fn player_settings_reach_the_session_and_preserve_silence() {
    let session = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(session);
    for volume in [0, 25, 50, 100, 255] {
        service.set_volume(volume).await.unwrap();
        assert_eq!(service.get_status().await.unwrap().volume, volume.min(100));
    }
    service.set_shuffle(true).await.unwrap();
    service.set_repeat(RepeatMode::Track).await.unwrap();
    let status = service.get_status().await.unwrap();
    assert!(status.shuffle);
    assert_eq!(status.repeat, RepeatMode::Track);
}

#[tokio::test]
async fn library_song_uses_apple_catalog_relationship_without_changing_identity() {
    let (base, shutdown) = spawn_mock_apple_api(vec![(200, serde_json::json!({
        "data": [{"id":"i.local", "relationships":{"catalog":{"data":[{"id":"12345", "type":"songs"}]}}}]
    }))]).await;
    let tokens = Arc::new(StaticTokenProvider::new(AppleCredentials::new(
        "dev", "user", "us",
    )));
    let api = OfficialAppleMusicApi::with_base_url(tokens, base);
    let library = MediaRef::Song("i.local".into());
    assert_eq!(
        api.catalog_reference(&library).await.unwrap(),
        MediaRef::Song("12345".into())
    );
    // A second call uses the cached relationship, not another request.
    assert_eq!(
        api.catalog_reference(&library).await.unwrap(),
        MediaRef::Song("12345".into())
    );
    assert_eq!(library.to_string(), "song:i.local");
    let _ = shutdown.send(());
}

#[tokio::test]
async fn collection_play_sets_requested_shuffle_and_preserves_resource_identity() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());
    service
        .play_collection(&MediaRef::Album("123".into()), true)
        .await
        .unwrap();
    assert!(*mock.shuffle.lock().await);
    assert_eq!(*mock.last_played_kind.lock().await, Some("album".into()));
    assert_eq!(*mock.last_played_track.lock().await, Some("123".into()));
    service
        .play_collection(&MediaRef::Playlist("p.actual".into()), false)
        .await
        .unwrap();
    assert!(!*mock.shuffle.lock().await);
    assert_eq!(*mock.last_played_kind.lock().await, Some("playlist".into()));
    assert_eq!(
        *mock.last_played_track.lock().await,
        Some("p.actual".into())
    );
}
