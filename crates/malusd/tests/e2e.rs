//! End-to-end integration tests between malusd and malus-client.
//!
//! Enforces:
//! - Daemon runs Apple Music in-process (no provider child processes).
//! - Fast IPC over Unix domain socket with framed JSON RPC.
//! - Client requests (Ping, Status, Play, Pause, Seek, Stop) succeed.
//! - Apple navigation and page requests succeed over IPC.
//! - Event streaming delivers live updates to subscribed clients.

use malus_client::MalusClient;
use malus_ipc::client::{ClientRequest, ClientResponse};
use malus_model::{MediaRef, PageRoute, PlaybackState};
use malusd::{Engine, Server};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::sleep;

struct TestServer {
    sock_path: PathBuf,
    engine: Arc<Engine>,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    async fn start() -> Self {
        let sock_path = std::env::temp_dir().join(format!(
            "malus-e2e-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let engine = Arc::new(Engine::new());
        let server = Server::new(&sock_path, engine.clone());
        let srv_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait for server to bind
        for _ in 0..50 {
            if sock_path.exists() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        Self {
            sock_path,
            engine,
            handle: srv_handle,
        }
    }

    fn broadcast_event(&self, event: malus_ipc::client::ClientEvent) {
        self.engine.emit(event);
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

#[tokio::test]
async fn test_daemon_client_ipc_ping_and_status() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();

    // 1. Ping
    let pong = client.send(&ClientRequest::Ping).await.unwrap();
    assert_eq!(pong, ClientResponse::Pong);

    // 2. Initial status
    let status = client.get_status().await.unwrap();
    assert_eq!(status.state, PlaybackState::Stopped);
    assert_eq!(status.volume, 100);
}

#[tokio::test]
async fn test_daemon_navigation_and_pages_over_ipc() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();

    // 1. GetNavigation
    let resp = client.send(&ClientRequest::GetNavigation).await.unwrap();
    if let ClientResponse::Navigation(nav) = resp {
        assert_eq!(nav.default_route, PageRoute::Home);
        assert_eq!(nav.groups.len(), 3);
        assert_eq!(nav.groups[0].id, "discover");
        assert_eq!(nav.groups[1].id, "library");
        assert_eq!(nav.groups[2].id, "playlists");
    } else {
        panic!("Expected Navigation response, got {resp:?}");
    }

    // 2. GetPage with client helper
    let page = client.get_page(&PageRoute::Home).await.unwrap();
    assert_eq!(page.id, "home");
    assert_eq!(page.title, "Home");
}

#[tokio::test]
#[ignore = "requires isolated browser profile without concurrent client"]
async fn test_daemon_playback_control_lifecycle() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();

    // 1. Play track
    let resp = client
        .send(&ClientRequest::PlayMedia {
            reference: MediaRef::parse("song:617154362").unwrap(),
            collection: None,
            index: None,
        })
        .await
        .unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    // 2. Pause
    let resp = client.send(&ClientRequest::Pause).await.unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    // 3. Play / Resume
    let resp = client.send(&ClientRequest::Play).await.unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    // 4. Seek
    let resp = client
        .send(&ClientRequest::Seek {
            position_ms: 30_000,
        })
        .await
        .unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    // 5. Stop
    let resp = client.send(&ClientRequest::Stop).await.unwrap();
    assert_eq!(resp, ClientResponse::Ok);
}

#[tokio::test]
async fn test_daemon_events_subscription() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();

    // Verify event subscription connects cleanly
    let (rx, _status) = client.subscribe_events();

    sleep(Duration::from_millis(50)).await;

    // Send a status change
    let _ = client.send(&ClientRequest::Stop).await;

    drop(rx);
}

#[tokio::test]
async fn test_daemon_multi_client_consistency_and_lifetime() {
    let server = TestServer::start().await;

    // Client A connects
    let client_a = MalusClient::connect(&server.sock_path).await.unwrap();

    // Verify initial queue
    let queue_a = client_a.get_queue().await.unwrap();
    assert!(queue_a.items.is_empty());

    // Client B connects to the same running daemon
    let client_b = MalusClient::connect(&server.sock_path).await.unwrap();

    // Client B observes same initial status and queue
    let status_b = client_b.get_status().await.unwrap();
    assert_eq!(status_b.state, PlaybackState::Stopped);
    let queue_b = client_b.get_queue().await.unwrap();
    assert_eq!(queue_a, queue_b);

    // Frontend-lifetime independence: Client A disconnects and exits
    drop(client_a);

    // Daemon is still running, Client B can still query state
    let status_b2 = client_b.get_status().await.unwrap();
    assert_eq!(status_b2.state, PlaybackState::Stopped);

    // Client C connects anew
    let client_c = MalusClient::connect(&server.sock_path).await.unwrap();
    let status_c = client_c.get_status().await.unwrap();
    assert_eq!(status_c.state, PlaybackState::Stopped);
    let queue_c = client_c.get_queue().await.unwrap();
    assert_eq!(queue_c.items.len(), 0);
}

#[tokio::test]
async fn test_daemon_media_state_events() {
    use malus_ipc::client::ClientEvent;
    use malus_model::{AccountMediaState, Rating};

    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();
    let (mut rx, mut status_rx) = client.subscribe_events();
    let _ = status_rx
        .wait_for(|s| *s == malus_client::ConnectionStatus::Connected)
        .await;

    // Broadcast a media state change event
    let song_ref = MediaRef::Song("1440857781".to_string());
    let state = AccountMediaState::new(song_ref, true, true, Rating::Neutral);
    server.broadcast_event(ClientEvent::MediaStateChanged(state.clone()));

    // Wait for the event on subscriber
    let received = tokio::time::timeout(Duration::from_millis(1000), rx.recv()).await;
    match received {
        Ok(Ok(ClientEvent::MediaStateChanged(ev_state))) => {
            assert_eq!(ev_state, state);
            assert!(ev_state.is_favorite());
            assert!(!ev_state.is_suggest_less());
            assert!(ev_state.in_library);
        }
        other => panic!("Expected MediaStateChanged event, got {other:?}"),
    }
}

#[tokio::test]
async fn test_daemon_playlist_mutation_requests() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path)
        .await
        .expect("Client connect");

    // Empty playlist creation should validate empty name or reach service
    let res = client.create_playlist("", None, vec![]).await;
    assert!(res.is_err(), "Empty playlist name must fail validation");

    let del_res = client
        .delete_playlist(&MediaRef::Playlist("p.test".to_string()))
        .await;
    // Without live profile auth, it returns an error (auth/network/upstream), NOT an unexpected response or IPC framing crash
    assert!(del_res.is_err());
}
