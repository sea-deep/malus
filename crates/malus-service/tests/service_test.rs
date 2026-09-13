use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_ipc::{
    PlaybackStateWire, PlayerStatusWire, RepeatModeWire,
    wire::{AuthStateWire, CatalogItemWire, LibraryKindWire, LibraryPageWire, TrackWire},
};
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
    is_paused: Mutex<bool>,
    is_stopped: Mutex<bool>,
    last_seek_ms: Mutex<Option<u64>>,
    event_sink: Mutex<Option<mpsc::UnboundedSender<PlayerStatusWire>>>,
}

impl MockAppleWebSession {
    fn new() -> Self {
        Self {
            probe_result: Mutex::new(Ok(AuthState::NeedsAuth)),
            begin_result: Mutex::new(Ok(AuthState::Authenticated)),
            logout_called: Mutex::new(false),
            last_played_track: Mutex::new(None),
            is_paused: Mutex::new(false),
            is_stopped: Mutex::new(true),
            last_seek_ms: Mutex::new(None),
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

    async fn play_track(&self, catalog_id: &str) -> Result<(), AppleError> {
        *self.last_played_track.lock().await = Some(catalog_id.to_string());
        *self.is_stopped.lock().await = false;
        *self.is_paused.lock().await = false;

        let guard = self.event_sink.lock().await;
        if let Some(ref sink) = *guard {
            let _ = sink.send(PlayerStatusWire {
                state: PlaybackStateWire::Playing,
                current_track: Some(TrackWire::new(
                    format!("song:{catalog_id}"),
                    "Mock Apple Song",
                    "Mock Artist",
                )),
                position_ms: 0,
                duration_ms: 180_000,
                volume: 100,
                muted: false,
                shuffle: false,
                repeat: RepeatModeWire::Off,
            });
        }
        Ok(())
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

    async fn get_status(&self) -> Result<PlayerStatusWire, AppleError> {
        let paused = *self.is_paused.lock().await;
        let stopped = *self.is_stopped.lock().await;
        let track_id = self.last_played_track.lock().await.clone();

        let state = if stopped {
            PlaybackStateWire::Stopped
        } else if paused {
            PlaybackStateWire::Paused
        } else {
            PlaybackStateWire::Playing
        };

        let current_track = track_id
            .map(|id| TrackWire::new(format!("song:{id}"), "Mock Apple Song", "Mock Artist"));

        Ok(PlayerStatusWire {
            state,
            current_track,
            position_ms: self.last_seek_ms.lock().await.unwrap_or(0),
            duration_ms: 180_000,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        })
    }

    fn set_event_sink(&self, sink: mpsc::UnboundedSender<PlayerStatusWire>) {
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

    service.play("song:1440857781").await.expect("play track");
    assert_eq!(
        mock.last_played_track.lock().await.as_deref(),
        Some("1440857781")
    );

    let status = service.get_status().await.expect("get status");
    assert_eq!(status.state, PlaybackStateWire::Playing);
    assert_eq!(
        status.current_track.as_ref().map(|t| t.id.as_str()),
        Some("song:1440857781")
    );
}

#[tokio::test]
async fn test_playback_play_invalid_kind() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock);

    // Wrong kind
    let err = service.play("album:12345").await.expect_err("wrong kind");
    assert!(
        err.to_string()
            .contains("Expected item kind 'song' or 'track'")
    );
}

#[tokio::test]
async fn test_playback_controls_forwarding() {
    let mock = Arc::new(MockAppleWebSession::new());
    let service = AppleService::with_session(mock.clone());

    service.play("song:123").await.unwrap();
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
    assert_eq!(tracks.items[0].id, "song:123");
    assert_eq!(tracks.items[0].title, "Daft Punk Song");

    let albums = search_res.albums.expect("albums");
    assert_eq!(albums.items.len(), 1);
    assert_eq!(albums.items[0].id, "album:456");

    // 2. Get Catalog Item (track)
    let item = service
        .get_catalog_item("song:123")
        .await
        .expect("track item");
    match item {
        CatalogItemWire::Track(t) => {
            assert_eq!(t.id, "song:123");
            assert_eq!(t.title, "Mock Track");
        }
        _ => panic!("Expected Track"),
    }

    // 3. Get Catalog Item (album)
    let item = service
        .get_catalog_item("album:456")
        .await
        .expect("album item");
    match item {
        CatalogItemWire::Album(a) => {
            assert_eq!(a.id, "album:456");
            assert_eq!(a.title, "Mock Album");
            assert_eq!(a.track_count, Some(12));
        }
        _ => panic!("Expected Album"),
    }

    // 4. Get Catalog Item (artist)
    let item = service
        .get_catalog_item("artist:789")
        .await
        .expect("artist item");
    match item {
        CatalogItemWire::Artist(a) => {
            assert_eq!(a.id, "artist:789");
            assert_eq!(a.name, "Mock Artist");
        }
        _ => panic!("Expected Artist"),
    }

    // 5. Get Catalog Item (playlist)
    let item = service
        .get_catalog_item("playlist:pl.abc")
        .await
        .expect("playlist item");
    match item {
        CatalogItemWire::Playlist(p) => {
            assert_eq!(p.id, "playlist:pl.abc");
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
        .get_collection_items("album:456", 20, None)
        .await
        .expect("collection items");
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, "song:101");
    assert_eq!(page.items[1].id, "song:102");

    // 2. Library tracks
    let lib_tracks = service
        .get_library(LibraryKindWire::Tracks, 20, None)
        .await
        .expect("library tracks");
    match lib_tracks {
        LibraryPageWire::Tracks(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, "song:i.123");
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
            assert_eq!(page.items[0].id, "album:l.456");
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
            assert_eq!(page.items[0].id, "playlist:p.789");
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
    assert_eq!(track.id, "song:1440857781");
    assert_eq!(track.title, "Get Lucky");
    assert_eq!(track.artists.len(), 1);
    assert_eq!(track.artists[0].id.as_deref(), Some("artist:5468295"));
    assert_eq!(track.artists[0].name, "Daft Punk");
    assert_eq!(
        track.album.as_ref().and_then(|a| a.id.as_deref()),
        Some("album:1440857780")
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
    assert_eq!(album.id, "album:1440857780");
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
    assert_eq!(artist.id, "artist:5468295");
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
    assert_eq!(playlist.id, "playlist:pl.u-xyz");
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
    assert_eq!(nav.default_route, "home");
    assert_eq!(nav.groups.len(), 3);

    // Discover group
    assert_eq!(nav.groups[0].id, "discover");
    assert_eq!(nav.groups[0].entries.len(), 3);
    assert_eq!(nav.groups[0].entries[0].route, "home");
    assert_eq!(nav.groups[0].entries[1].route, "new");
    assert_eq!(nav.groups[0].entries[2].route, "radio");

    // Library group
    assert_eq!(nav.groups[1].id, "library");
    assert_eq!(nav.groups[1].entries.len(), 5);
    assert_eq!(nav.groups[1].entries[0].route, "library:recently-added");
    assert_eq!(nav.groups[1].entries[1].route, "library:artists");
    assert_eq!(nav.groups[1].entries[2].route, "library:albums");
    assert_eq!(nav.groups[1].entries[3].route, "library:songs");
    assert_eq!(nav.groups[1].entries[4].route, "library:playlists");

    // Replay group
    assert_eq!(nav.groups[2].id, "replay");
    assert!(!nav.groups[2].entries.is_empty());
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

    let page = service.get_page("home").await.expect("home page");
    assert_eq!(page.id, "home");
    assert_eq!(page.title, "Listen Now");
    assert_eq!(page.sections.len(), 3);

    // Section 1: Recommendations
    assert_eq!(page.sections[0].title.as_deref(), Some("Favorites Mix"));
    assert_eq!(page.sections[0].items.len(), 1);
    assert_eq!(page.sections[0].items[0].id, "pl.fav");
    assert_eq!(
        page.sections[0].items[0].entity,
        Some(malus_ipc::wire::MediaRefWire::parse("playlist:pl.fav").unwrap())
    );
    assert_eq!(
        page.sections[0].items[0].open_route.as_deref(),
        Some("playlist:pl.fav")
    );

    // Section 2: Recently Played
    assert_eq!(page.sections[1].id, "recently-played");
    assert_eq!(page.sections[1].items.len(), 1);
    assert_eq!(page.sections[1].items[0].id, "1440857781");
    assert_eq!(
        page.sections[1].items[0].entity,
        Some(malus_ipc::wire::MediaRefWire::parse("song:1440857781").unwrap())
    );

    // Section 3: Heavy Rotation
    assert_eq!(page.sections[2].id, "heavy-rotation");
    assert_eq!(page.sections[2].items.len(), 1);
    assert_eq!(page.sections[2].items[0].id, "1440857780");
    assert_eq!(
        page.sections[2].items[0].entity,
        Some(malus_ipc::wire::MediaRefWire::parse("album:1440857780").unwrap())
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
        .get_page("album:1440857780")
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
        Some(malus_ipc::wire::MediaRefWire::parse("song:1440857781").unwrap())
    );

    // Playlist detail
    let playlist_page = service
        .get_page("playlist:pl.123")
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
        Some(malus_ipc::wire::MediaRefWire::parse("song:1440857781").unwrap())
    );
}
