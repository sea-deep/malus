use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_protocol::{
    PlaybackStateWire, PlayerStatusWire, RepeatModeWire,
    provider::ProviderEvent,
    wire::{
        AlbumWire, ArtistWire, AuthStateWire, CatalogItemWire, LibraryKindWire, LibraryPageWire,
        PageWire, PlaylistWire, SearchKindWire, SearchResultsWire, TrackWire,
    },
};
use malus_provider_apple::{
    AppleError, AppleProvider, AppleWebSession, AuthState, parse_apple_album, parse_apple_artist,
    parse_apple_playlist, parse_apple_track,
};
use malus_provider_sdk::{
    Provider,
    capability::{
        AUTH, AUTH_BROWSER, CATALOG_ALBUM, CATALOG_ARTIST, CATALOG_PLAYLIST, CATALOG_TRACK,
        LIBRARY_ALBUMS, LIBRARY_PLAYLISTS, LIBRARY_TRACKS, PLAYBACK, PLAYBACK_SEEK, SEARCH,
    },
};
use tokio::sync::{Mutex, mpsc};

struct MockAppleWebSession {
    probe_result: Mutex<Result<AuthState, AppleError>>,
    begin_result: Mutex<Result<AuthState, AppleError>>,
    logout_result: Mutex<Result<(), AppleError>>,
    logout_called: Mutex<bool>,
    last_played_track: Mutex<Option<String>>,
    is_paused: Mutex<bool>,
    is_stopped: Mutex<bool>,
    last_seek_ms: Mutex<Option<u64>>,
    event_sink: Mutex<Option<mpsc::UnboundedSender<ProviderEvent>>>,
}

impl MockAppleWebSession {
    fn new() -> Self {
        Self {
            probe_result: Mutex::new(Ok(AuthState::NeedsAuth)),
            begin_result: Mutex::new(Ok(AuthState::Authenticated)),
            logout_result: Mutex::new(Ok(())),
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
            Err(e) => Err(match e {
                AppleError::ProfileBusy => AppleError::ProfileBusy,
                AppleError::AuthCancelled => AppleError::AuthCancelled,
                AppleError::AuthTimeout => AppleError::AuthTimeout,
                AppleError::MusicKitUnavailable => AppleError::MusicKitUnavailable,
                AppleError::BrowserDisconnected => AppleError::BrowserDisconnected,
                _ => AppleError::AuthTimeout,
            }),
        }
    }

    async fn begin_auth(&self, _timeout: Duration) -> Result<AuthState, AppleError> {
        let guard = self.begin_result.lock().await;
        match &*guard {
            Ok(s) => Ok(*s),
            Err(e) => Err(match e {
                AppleError::ProfileBusy => AppleError::ProfileBusy,
                AppleError::AuthCancelled => AppleError::AuthCancelled,
                AppleError::AuthTimeout => AppleError::AuthTimeout,
                AppleError::MusicKitUnavailable => AppleError::MusicKitUnavailable,
                AppleError::BrowserDisconnected => AppleError::BrowserDisconnected,
                _ => AppleError::AuthTimeout,
            }),
        }
    }

    async fn logout(&self) -> Result<(), AppleError> {
        *self.logout_called.lock().await = true;
        let guard = self.logout_result.lock().await;
        match &*guard {
            Ok(()) => Ok(()),
            Err(_) => Err(AppleError::AuthCancelled),
        }
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
            let _ = sink.send(ProviderEvent::StatusChanged(PlayerStatusWire {
                state: PlaybackStateWire::Playing,
                current_track: Some(TrackWire::new(
                    format!("apple:track:{catalog_id}"),
                    "Mock Apple Song",
                    "Mock Artist",
                )),
                position_ms: 0,
                duration_ms: 180_000,
                volume: 100,
                muted: false,
                shuffle: false,
                repeat: RepeatModeWire::Off,
            }));
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

        let current_track = track_id.map(|id| {
            TrackWire::new(
                format!("apple:track:{id}"),
                "Mock Apple Song",
                "Mock Artist",
            )
        });

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

    fn set_event_sink(&self, sink: mpsc::UnboundedSender<ProviderEvent>) {
        if let Ok(mut guard) = self.event_sink.try_lock() {
            *guard = Some(sink);
        }
    }

    async fn search(
        &self,
        query: &str,
        _kinds: &[SearchKindWire],
        _limit: usize,
        _cursor: Option<&str>,
    ) -> Result<SearchResultsWire, AppleError> {
        let track = TrackWire::new("apple:track:123", format!("{query} Song"), "Test Artist");
        let album = AlbumWire {
            id: "apple:album:456".into(),
            title: format!("{query} Album"),
            artists: vec![],
            track_count: Some(10),
            release_date: None,
            artwork: None,
        };
        Ok(SearchResultsWire {
            tracks: Some(PageWire::new(vec![track], None)),
            albums: Some(PageWire::new(vec![album], None)),
            artists: None,
            playlists: None,
        })
    }

    async fn get_catalog_item(&self, media_id: &str) -> Result<CatalogItemWire, AppleError> {
        let mid = malus_protocol::MediaIdWire::parse(media_id)
            .map_err(|e| AppleError::Internal(e.to_string()))?;
        match mid.kind() {
            "track" => Ok(CatalogItemWire::Track(TrackWire::new(
                media_id,
                "Mock Track",
                "Mock Artist",
            ))),
            "album" => Ok(CatalogItemWire::Album(AlbumWire {
                id: media_id.to_string(),
                title: "Mock Album".into(),
                artists: vec![],
                track_count: Some(12),
                release_date: None,
                artwork: None,
            })),
            "artist" => Ok(CatalogItemWire::Artist(ArtistWire {
                id: media_id.to_string(),
                name: "Mock Artist".into(),
                artwork: None,
            })),
            "playlist" => Ok(CatalogItemWire::Playlist(PlaylistWire {
                id: media_id.to_string(),
                title: "Mock Playlist".into(),
                curator: Some("Apple Music".into()),
                description: None,
                track_count: Some(50),
                artwork: None,
            })),
            other => Err(AppleError::NotFound(format!("Unknown kind {other}"))),
        }
    }

    async fn get_collection_items(
        &self,
        _media_id: &str,
        _limit: usize,
        _cursor: Option<&str>,
    ) -> Result<PageWire<TrackWire>, AppleError> {
        let t1 = TrackWire::new("apple:track:101", "Collection Song 1", "Collection Artist");
        let t2 = TrackWire::new("apple:track:102", "Collection Song 2", "Collection Artist");
        Ok(PageWire::new(vec![t1, t2], None))
    }

    async fn get_library(
        &self,
        kind: LibraryKindWire,
        _limit: usize,
        _cursor: Option<&str>,
    ) -> Result<LibraryPageWire, AppleError> {
        match kind {
            LibraryKindWire::Tracks => {
                let t = TrackWire::new("apple:track:i.123", "Library Song", "Library Artist");
                Ok(LibraryPageWire::Tracks(PageWire::new(vec![t], None)))
            }
            LibraryKindWire::Albums => {
                let a = AlbumWire {
                    id: "apple:album:l.456".into(),
                    title: "Library Album".into(),
                    artists: vec![],
                    track_count: Some(8),
                    release_date: None,
                    artwork: None,
                };
                Ok(LibraryPageWire::Albums(PageWire::new(vec![a], None)))
            }
            LibraryKindWire::Playlists => {
                let p = PlaylistWire {
                    id: "apple:playlist:p.789".into(),
                    title: "Library Playlist".into(),
                    curator: None,
                    description: None,
                    track_count: Some(20),
                    artwork: None,
                };
                Ok(LibraryPageWire::Playlists(PageWire::new(vec![p], None)))
            }
        }
    }
}

#[tokio::test]
async fn test_provider_identity_and_capabilities() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock);

    assert_eq!(provider.id(), "apple");
    assert_eq!(provider.name(), "Apple Music");
    assert_eq!(
        provider.capabilities(),
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

    let provider = AppleProvider::with_session(mock.clone());
    let status = provider.auth_status().await.expect("probe auth status");

    assert_eq!(status.provider, "apple");
    assert_eq!(status.state, AuthStateWire::NeedsAuth);

    // Simulate returning Authenticated on subsequent probe
    *mock.probe_result.lock().await = Ok(AuthState::Authenticated);
    let status = provider.auth_status().await.expect("probe auth status");
    assert_eq!(status.state, AuthStateWire::Authenticated);
}

#[tokio::test]
async fn test_auth_begin_success() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Ok(AuthState::Authenticated);

    let provider = AppleProvider::with_session(mock);
    let status = provider.auth_begin().await.expect("auth begin");

    assert_eq!(status.provider, "apple");
    assert_eq!(status.state, AuthStateWire::Authenticated);
}

#[tokio::test]
async fn test_auth_begin_cancellation() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::AuthCancelled);

    let provider = AppleProvider::with_session(mock);
    let err = provider.auth_begin().await.expect_err("should cancel");

    assert!(err.to_string().contains("closed by user"));
}

#[tokio::test]
async fn test_auth_begin_timeout() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::AuthTimeout);

    let provider = AppleProvider::with_session(mock);
    let err = provider.auth_begin().await.expect_err("should time out");

    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn test_auth_begin_profile_busy() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::ProfileBusy);

    let provider = AppleProvider::with_session(mock);
    let err = provider
        .auth_begin()
        .await
        .expect_err("should fail if busy");

    assert!(err.to_string().contains("already in use"));
}

#[tokio::test]
async fn test_auth_logout() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    let status = provider.auth_logout().await.expect("logout");
    assert_eq!(status.state, AuthStateWire::NeedsAuth);
    assert!(*mock.logout_called.lock().await);
}

#[tokio::test]
async fn test_playback_play_valid_media_id() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    provider
        .play("apple:track:1440857781")
        .await
        .expect("play track");
    assert_eq!(
        mock.last_played_track.lock().await.as_deref(),
        Some("1440857781")
    );

    let status = provider.get_status().await.expect("get status");
    assert_eq!(status.state, PlaybackStateWire::Playing);
    assert_eq!(
        status.current_track.as_ref().map(|t| t.id.as_str()),
        Some("apple:track:1440857781")
    );
}

#[tokio::test]
async fn test_playback_play_invalid_provider_or_kind() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock);

    // Wrong provider
    let err = provider
        .play("mock:track:1")
        .await
        .expect_err("wrong provider");
    assert!(err.to_string().contains("Expected provider 'apple'"));

    // Wrong kind
    let err = provider
        .play("apple:album:12345")
        .await
        .expect_err("wrong kind");
    assert!(err.to_string().contains("Expected item kind 'track'"));
}

#[tokio::test]
async fn test_playback_controls_forwarding() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    provider.play("apple:track:123").await.unwrap();
    assert!(!*mock.is_paused.lock().await);

    provider.pause().await.unwrap();
    assert!(*mock.is_paused.lock().await);

    provider.resume().await.unwrap();
    assert!(!*mock.is_paused.lock().await);

    provider.seek(45_000).await.unwrap();
    assert_eq!(*mock.last_seek_ms.lock().await, Some(45_000));

    provider.stop().await.unwrap();
    assert!(*mock.is_stopped.lock().await);
}

#[tokio::test]
async fn test_apple_search_and_catalog() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    // 1. Search
    let search_res = provider
        .search("Daft Punk", &[], 10, None)
        .await
        .expect("search");
    let tracks = search_res.tracks.expect("tracks");
    assert_eq!(tracks.items.len(), 1);
    assert_eq!(tracks.items[0].id, "apple:track:123");
    assert_eq!(tracks.items[0].title, "Daft Punk Song");

    let albums = search_res.albums.expect("albums");
    assert_eq!(albums.items.len(), 1);
    assert_eq!(albums.items[0].id, "apple:album:456");

    // 2. Get Catalog Item (track)
    let item = provider
        .get_catalog_item("apple:track:123")
        .await
        .expect("track item");
    match item {
        CatalogItemWire::Track(t) => {
            assert_eq!(t.id, "apple:track:123");
            assert_eq!(t.title, "Mock Track");
        }
        _ => panic!("Expected Track"),
    }

    // 3. Get Catalog Item (album)
    let item = provider
        .get_catalog_item("apple:album:456")
        .await
        .expect("album item");
    match item {
        CatalogItemWire::Album(a) => {
            assert_eq!(a.id, "apple:album:456");
            assert_eq!(a.title, "Mock Album");
            assert_eq!(a.track_count, Some(12));
        }
        _ => panic!("Expected Album"),
    }

    // 4. Get Catalog Item (artist)
    let item = provider
        .get_catalog_item("apple:artist:789")
        .await
        .expect("artist item");
    match item {
        CatalogItemWire::Artist(a) => {
            assert_eq!(a.id, "apple:artist:789");
            assert_eq!(a.name, "Mock Artist");
        }
        _ => panic!("Expected Artist"),
    }

    // 5. Get Catalog Item (playlist)
    let item = provider
        .get_catalog_item("apple:playlist:pl.abc")
        .await
        .expect("playlist item");
    match item {
        CatalogItemWire::Playlist(p) => {
            assert_eq!(p.id, "apple:playlist:pl.abc");
            assert_eq!(p.title, "Mock Playlist");
        }
        _ => panic!("Expected Playlist"),
    }
}

#[tokio::test]
async fn test_apple_collection_and_library() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    // 1. Collection items
    let page = provider
        .get_collection_items("apple:album:456", 20, None)
        .await
        .expect("collection items");
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, "apple:track:101");
    assert_eq!(page.items[1].id, "apple:track:102");

    // 2. Library tracks
    let lib_tracks = provider
        .get_library(LibraryKindWire::Tracks, 20, None)
        .await
        .expect("library tracks");
    match lib_tracks {
        LibraryPageWire::Tracks(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, "apple:track:i.123");
        }
        _ => panic!("Expected LibraryPageWire::Tracks"),
    }

    // 3. Library albums
    let lib_albums = provider
        .get_library(LibraryKindWire::Albums, 20, None)
        .await
        .expect("library albums");
    match lib_albums {
        LibraryPageWire::Albums(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, "apple:album:l.456");
        }
        _ => panic!("Expected LibraryPageWire::Albums"),
    }

    // 4. Library playlists
    let lib_playlists = provider
        .get_library(LibraryKindWire::Playlists, 20, None)
        .await
        .expect("library playlists");
    match lib_playlists {
        LibraryPageWire::Playlists(page) => {
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].id, "apple:playlist:p.789");
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
    assert_eq!(track.id, "apple:track:1440857781");
    assert_eq!(track.title, "Get Lucky");
    assert_eq!(track.artists.len(), 1);
    assert_eq!(track.artists[0].id.as_deref(), Some("apple:artist:5468295"));
    assert_eq!(track.artists[0].name, "Daft Punk");
    assert_eq!(
        track.album.as_ref().and_then(|a| a.id.as_deref()),
        Some("apple:album:1440857780")
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
    assert_eq!(album.id, "apple:album:1440857780");
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
    assert_eq!(artist.id, "apple:artist:5468295");
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
    assert_eq!(playlist.id, "apple:playlist:pl.u-xyz");
    assert_eq!(playlist.title, "Summer Vibes");
    assert_eq!(playlist.curator.as_deref(), Some("Apple Music Electronic"));
    assert_eq!(playlist.track_count, Some(42));
}
