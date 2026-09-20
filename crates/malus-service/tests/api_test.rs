use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use malus_ipc::wire::{CatalogItemWire, LibraryKindWire, LibraryPageWire, SearchKindWire};
use malus_model::MediaRef;
use malus_service::{
    AppleApiError, AppleCredentials, OfficialAppleMusicApi, StaticTokenProvider, parse_apple_album,
    parse_apple_artist, parse_apple_artwork, parse_apple_playlist, parse_apple_track,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// Simple mock HTTP server for testing OfficialAppleMusicApi.
struct MockHttpServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    _responses: Arc<Mutex<Vec<MockResponse>>>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path_and_query: String,
    headers: HashMap<String, String>,
}

#[derive(Clone)]
struct MockResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl MockHttpServer {
    async fn start(responses: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(responses));
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        let reqs_clone = requests.clone();
        let resps_clone = responses.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accept_res = listener.accept() => {
                        let (mut stream, _) = match accept_res {
                            Ok(pair) => pair,
                            Err(_) => break,
                        };

                        let reqs = reqs_clone.clone();
                        let resps = resps_clone.clone();

                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 4096];
                            let n = match stream.read(&mut buf).await {
                                Ok(n) if n > 0 => n,
                                _ => return,
                            };

                            let req_text = String::from_utf8_lossy(&buf[..n]);
                            let mut lines = req_text.lines();
                            let first_line = lines.next().unwrap_or("");
                            let mut parts = first_line.split_whitespace();
                            let method = parts.next().unwrap_or("").to_string();
                            let path_and_query = parts.next().unwrap_or("").to_string();

                            let mut headers = HashMap::new();
                            for line in lines {
                                if line.is_empty() {
                                    break;
                                }
                                if let Some((k, v)) = line.split_once(':') {
                                    headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                                }
                            }

                            reqs.lock().unwrap().push(RecordedRequest {
                                method,
                                path_and_query,
                                headers,
                            });

                            let resp = {
                                let mut r_guard = resps.lock().unwrap();
                                if !r_guard.is_empty() {
                                    r_guard.remove(0)
                                } else {
                                    MockResponse {
                                        status: 200,
                                        headers: vec![("Content-Type".into(), "application/json".into())],
                                        body: "{}".into(),
                                    }
                                }
                            };

                            let status_text = match resp.status {
                                200 => "OK",
                                401 => "Unauthorized",
                                403 => "Forbidden",
                                404 => "Not Found",
                                429 => "Too Many Requests",
                                500 => "Internal Server Error",
                                _ => "Custom",
                            };

                            let mut resp_str = format!(
                                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                                resp.status,
                                status_text,
                                resp.body.len()
                            );
                            for (k, v) in resp.headers {
                                resp_str.push_str(&format!("{k}: {v}\r\n"));
                            }
                            resp_str.push_str("\r\n");
                            resp_str.push_str(&resp.body);

                            let _ = stream.write_all(resp_str.as_bytes()).await;
                            let _ = stream.flush().await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            requests,
            _responses: responses,
            _shutdown: tx,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        // Signal shutdown if sender has not been consumed
        // (sender is consumed by sending, but if already dropped, ignore)
    }
}

#[tokio::test]
async fn test_credentials_debug_redaction() {
    let creds = AppleCredentials::new("secret-dev-token", "secret-user-token", "in");
    let debug_str = format!("{creds:?}");

    assert!(debug_str.contains("developer_token: \"[REDACTED]\""));
    assert!(debug_str.contains("music_user_token: \"[REDACTED]\""));
    assert!(debug_str.contains("storefront: \"in\""));
    assert!(!debug_str.contains("secret-dev-token"));
    assert!(!debug_str.contains("secret-user-token"));
}

#[tokio::test]
async fn test_request_headers_and_path_construction() {
    let resp = MockResponse {
        status: 200,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: serde_json::json!({
            "results": {
                "songs": {
                    "data": [
                        {
                            "id": "12345",
                            "type": "songs",
                            "attributes": {
                                "name": "Instant Crush",
                                "artistName": "Daft Punk",
                                "albumName": "Random Access Memories",
                                "durationInMillis": 337560
                            }
                        }
                    ]
                }
            }
        })
        .to_string(),
    };

    let server = MockHttpServer::start(vec![resp]).await;
    let creds = AppleCredentials::new("test-dev-token", "test-user-token", "in");
    let token_provider = Arc::new(StaticTokenProvider::new(creds));
    let api = OfficialAppleMusicApi::with_base_url(token_provider, server.url());

    let results = api
        .search("Daft Punk", &[SearchKindWire::Track], 5, None)
        .await
        .expect("search should succeed");

    let reqs = server.recorded_requests();
    assert_eq!(reqs.len(), 1);

    let req = &reqs[0];
    assert_eq!(req.method, "GET");
    // Verify storefront substitution: {storefront} -> in
    assert!(req.path_and_query.starts_with("/catalog/in/search?"));
    assert!(
        req.path_and_query.contains("term=Daft+Punk")
            || req.path_and_query.contains("term=Daft%20Punk")
    );
    assert!(req.path_and_query.contains("types=songs"));
    assert!(req.path_and_query.contains("limit=5"));

    // Verify required headers
    assert_eq!(
        req.headers.get("authorization").map(String::as_str),
        Some("Bearer test-dev-token")
    );
    assert_eq!(
        req.headers.get("music-user-token").map(String::as_str),
        Some("test-user-token")
    );
    assert_eq!(
        req.headers.get("origin").map(String::as_str),
        Some("https://music.apple.com")
    );
    assert_eq!(
        req.headers.get("referer").map(String::as_str),
        Some("https://music.apple.com/")
    );
    assert!(req.headers.contains_key("user-agent"));

    // Verify parsed search result
    let tracks = results.tracks.expect("tracks should be present");
    assert_eq!(tracks.items.len(), 1);
    assert_eq!(tracks.items[0].id, MediaRef::Song("12345".to_string()));
    assert_eq!(tracks.items[0].title, "Instant Crush");
    assert_eq!(tracks.items[0].artists[0].name, "Daft Punk");
}

#[tokio::test]
async fn test_auth_retry_on_401_success() {
    let resp_401 = MockResponse {
        status: 401,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: r#"{"errors":[{"status":"401","title":"Unauthorized"}]}"#.into(),
    };

    let resp_200 = MockResponse {
        status: 200,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: serde_json::json!({
            "data": [
                {
                    "id": "1440857780",
                    "type": "albums",
                    "attributes": {
                        "name": "Random Access Memories",
                        "artistName": "Daft Punk",
                        "trackCount": 13
                    }
                }
            ]
        })
        .to_string(),
    };

    let server = MockHttpServer::start(vec![resp_401, resp_200]).await;
    let initial_creds = AppleCredentials::new("stale-dev-token", "stale-user-token", "us");
    let counter = Arc::new(AtomicUsize::new(0));
    let token_provider = Arc::new(StaticTokenProvider::with_refresh_counter(
        initial_creds,
        counter.clone(),
    ));

    // Update credentials when refresh happens
    let provider_clone = token_provider.clone();
    let api = OfficialAppleMusicApi::with_base_url(token_provider, server.url());

    // When refresh_credentials is run, set new credentials
    provider_clone.set_next_credentials(AppleCredentials::new(
        "fresh-dev-token",
        "fresh-user-token",
        "us",
    ));

    let item = api
        .get_catalog_item(&MediaRef::Album("1440857780".to_string()))
        .await
        .expect("should succeed after 401 refresh");

    match item {
        CatalogItemWire::Album(alb) => {
            assert_eq!(alb.id, MediaRef::Album("1440857780".to_string()));
            assert_eq!(alb.title, "Random Access Memories");
        }
        _ => panic!("Expected album"),
    }

    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let reqs = server.recorded_requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(
        reqs[0].headers.get("authorization").map(String::as_str),
        Some("Bearer stale-dev-token")
    );
    assert_eq!(
        reqs[1].headers.get("authorization").map(String::as_str),
        Some("Bearer fresh-dev-token")
    );
}

#[tokio::test]
async fn test_persistent_401_returns_auth_required() {
    let resp_401_a = MockResponse {
        status: 401,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: r#"{"errors":[{"status":"401","title":"Unauthorized"}]}"#.into(),
    };
    let resp_401_b = MockResponse {
        status: 401,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: r#"{"errors":[{"status":"401","title":"Unauthorized"}]}"#.into(),
    };

    let server = MockHttpServer::start(vec![resp_401_a, resp_401_b]).await;
    let creds = AppleCredentials::new("token1", "token2", "us");
    let token_provider = Arc::new(StaticTokenProvider::new(creds));
    let api = OfficialAppleMusicApi::with_base_url(token_provider, server.url());

    let err = api
        .get_catalog_item(&MediaRef::Song("123".to_string()))
        .await
        .unwrap_err();

    match err {
        AppleApiError::AuthRequired(_) => {}
        other => panic!("Expected AuthRequired, got: {other:?}"),
    }

    let reqs = server.recorded_requests();
    assert_eq!(reqs.len(), 2);
}

#[tokio::test]
async fn test_error_status_mappings() {
    // 403 Forbidden
    {
        let server = MockHttpServer::start(vec![MockResponse {
            status: 403,
            headers: vec![],
            body: "Forbidden".into(),
        }])
        .await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );
        let err = api
            .get_catalog_item(&MediaRef::Song("1".to_string()))
            .await
            .unwrap_err();
        assert!(matches!(err, AppleApiError::Forbidden(_)));
    }

    // 404 Not Found
    {
        let server = MockHttpServer::start(vec![MockResponse {
            status: 404,
            headers: vec![],
            body: "Not Found".into(),
        }])
        .await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );
        let err = api
            .get_catalog_item(&MediaRef::Song("nonexistent".to_string()))
            .await
            .unwrap_err();
        assert!(matches!(err, AppleApiError::NotFound(_)));
    }

    // 429 Rate Limited with Retry-After
    {
        let server = MockHttpServer::start(vec![MockResponse {
            status: 429,
            headers: vec![("Retry-After".into(), "30".into())],
            body: "Too Many Requests".into(),
        }])
        .await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );
        let err = api
            .get_catalog_item(&MediaRef::Song("1".to_string()))
            .await
            .unwrap_err();
        match err {
            AppleApiError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(30)));
            }
            other => panic!("Expected RateLimited, got: {other:?}"),
        }
    }

    // 500 Server Error
    {
        let server = MockHttpServer::start(vec![MockResponse {
            status: 500,
            headers: vec![],
            body: "Internal Server Error".into(),
        }])
        .await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );
        let err = api
            .get_catalog_item(&MediaRef::Song("1".to_string()))
            .await
            .unwrap_err();
        match err {
            AppleApiError::Server { status, .. } => assert_eq!(status, 500),
            other => panic!("Expected Server error, got: {other:?}"),
        }
    }

    // Malformed JSON response
    {
        let server = MockHttpServer::start(vec![MockResponse {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: "not-json-content{".into(),
        }])
        .await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );
        let err = api
            .get_catalog_item(&MediaRef::Song("1".to_string()))
            .await
            .unwrap_err();
        assert!(matches!(err, AppleApiError::Parse(_)));
    }
}

#[tokio::test]
async fn test_collection_and_library_calls() {
    // 1. Collection items (catalog album tracks)
    {
        let resp = MockResponse {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: serde_json::json!({
                "data": [
                    {
                        "id": "1001",
                        "type": "songs",
                        "attributes": {
                            "name": "Give Life Back to Music",
                            "artistName": "Daft Punk",
                            "trackNumber": 1
                        }
                    },
                    {
                        "id": "1002",
                        "type": "songs",
                        "attributes": {
                            "name": "The Game of Love",
                            "artistName": "Daft Punk",
                            "trackNumber": 2
                        }
                    }
                ],
                "next": "/v1/catalog/us/albums/1440857780/tracks?offset=2"
            })
            .to_string(),
        };

        let server = MockHttpServer::start(vec![resp]).await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );

        let page = api
            .get_collection_items(&MediaRef::Album("1440857780".to_string()), 2, None)
            .await
            .expect("collection items");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, MediaRef::Song("1001".to_string()));
        assert_eq!(page.items[0].title, "Give Life Back to Music");
        assert_eq!(page.items[1].id, MediaRef::Song("1002".to_string()));
        assert_eq!(
            page.next_cursor.as_deref(),
            Some("/v1/catalog/us/albums/1440857780/tracks?offset=2")
        );
    }

    // 2. Library songs
    {
        let resp = MockResponse {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: serde_json::json!({
                "data": [
                    {
                        "id": "i.123456",
                        "type": "library-songs",
                        "attributes": {
                            "name": "My Library Song",
                            "artistName": "Library Artist"
                        }
                    }
                ]
            })
            .to_string(),
        };

        let server = MockHttpServer::start(vec![resp]).await;
        let api = OfficialAppleMusicApi::with_base_url(
            Arc::new(StaticTokenProvider::new(AppleCredentials::new(
                "d", "u", "us",
            ))),
            server.url(),
        );

        let page = api
            .get_library(LibraryKindWire::Tracks, 10, None)
            .await
            .expect("library songs");

        match page {
            LibraryPageWire::Tracks(p) => {
                assert_eq!(p.items.len(), 1);
                assert_eq!(p.items[0].id, MediaRef::Song("i.123456".to_string()));
                assert_eq!(p.items[0].title, "My Library Song");
            }
            _ => panic!("Expected Tracks"),
        }

        let reqs = server.recorded_requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].path_and_query.starts_with("/me/library/songs?"));
    }
}

#[test]
fn test_canonical_parsers() {
    let art_json = serde_json::json!({
        "url": "https://example.com/art/{w}x{h}bb.jpg",
        "width": 1400,
        "height": 1400
    });
    let art = parse_apple_artwork(&art_json).expect("artwork");
    assert_eq!(art.url, "https://example.com/art/600x600bb.jpg");
    assert_eq!(art.width, Some(1400));
    assert_eq!(art.height, Some(1400));

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
            "contentRating": "explicit",
            "artwork": { "url": "https://example.com/art/{w}x{h}.jpg", "width": 600, "height": 600 }
        }
    });
    let track = parse_apple_track(&track_json).expect("track");
    assert_eq!(track.id, MediaRef::Song("1440857781".to_string()));
    assert_eq!(track.title, "Get Lucky");
    assert_eq!(track.duration_ms, Some(369626));
    assert_eq!(track.explicit, Some(true));

    let album_json = serde_json::json!({
        "id": "1440857780",
        "type": "albums",
        "attributes": {
            "name": "Random Access Memories",
            "artistName": "Daft Punk",
            "trackCount": 13,
            "releaseDate": "2013-05-17"
        }
    });
    let album = parse_apple_album(&album_json).expect("album");
    assert_eq!(album.id, MediaRef::Album("1440857780".to_string()));
    assert_eq!(album.title, "Random Access Memories");
    assert_eq!(album.track_count, Some(13));

    let artist_json = serde_json::json!({
        "id": "5468295",
        "type": "artists",
        "attributes": { "name": "Daft Punk" }
    });
    let artist = parse_apple_artist(&artist_json).expect("artist");
    assert_eq!(artist.id, MediaRef::Artist("5468295".to_string()));
    assert_eq!(artist.name, "Daft Punk");

    let playlist_json = serde_json::json!({
        "id": "pl.u-1234",
        "type": "playlists",
        "attributes": {
            "name": "Electronic Hits",
            "curatorName": "Apple Music",
            "trackCount": 50
        }
    });
    let playlist = parse_apple_playlist(&playlist_json).expect("playlist");
    assert_eq!(playlist.id, MediaRef::Playlist("pl.u-1234".to_string()));
    assert_eq!(playlist.title, "Electronic Hits");
    assert_eq!(playlist.curator.as_deref(), Some("Apple Music"));
}

#[test]
fn test_parse_apple_track_collab_and_relationships() {
    // 1. Direct relationships artists & albums
    let track_json = serde_json::json!({
        "id": "1753765105",
        "type": "songs",
        "attributes": {
            "name": "Radha",
            "artistName": "Natkhat & Chaar Diwaari"
        },
        "relationships": {
            "artists": {
                "data": [
                    { "id": "1560945939", "type": "artists", "attributes": { "name": "Natkhat" } },
                    { "id": "1612345678", "type": "artists", "attributes": { "name": "Chaar Diwaari" } }
                ]
            },
            "albums": {
                "data": [
                    { "id": "1753765104", "type": "albums", "attributes": { "name": "Radha - Single" } }
                ]
            }
        }
    });

    let track = parse_apple_track(&track_json).expect("track");
    assert_eq!(track.artists.len(), 2);
    assert_eq!(track.artists[0].name, "Natkhat");
    assert_eq!(
        track.artists[0].id,
        Some(MediaRef::Artist("1560945939".to_string()))
    );
    assert_eq!(track.artists[1].name, "Chaar Diwaari");
    assert_eq!(
        track.artists[1].id,
        Some(MediaRef::Artist("1612345678".to_string()))
    );
    assert_eq!(
        track.album.as_ref().and_then(|a| a.id.as_ref()),
        Some(&MediaRef::Album("1753765104".to_string()))
    );

    // 2. Fallback tokenization when relationships are absent
    let raw_collab = serde_json::json!({
        "id": "999",
        "type": "songs",
        "attributes": {
            "name": "Collab Song",
            "artistName": "Artist A & Artist B"
        }
    });
    let track_fallback = parse_apple_track(&raw_collab).expect("track");
    assert_eq!(track_fallback.artists.len(), 2);
    assert_eq!(track_fallback.artists[0].name, "Artist A");
    assert!(track_fallback.artists[0].id.is_none());
    assert_eq!(track_fallback.artists[1].name, "Artist B");
    assert!(track_fallback.artists[1].id.is_none());
}
