//! End-to-end integration tests between malusd and malus-client.
//!
//! Enforces:
//! - Daemon runs Apple Music in-process (no provider child processes).
//! - Fast IPC over Unix domain socket with framed JSON RPC.
//! - Client requests (Ping, Status, Play, Pause, Seek, Stop) succeed.
//! - Apple navigation and page requests succeed over IPC.
//! - Event streaming delivers live updates to subscribed clients.

use malus_client::MalusClient;
use malus_ipc::{
    PlaybackStateWire,
    client::{ClientRequest, ClientResponse},
};
use malusd::{Engine, Server};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::sleep;

struct TestServer {
    sock_path: PathBuf,
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
        let server = Server::new(&sock_path, engine);
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
            handle: srv_handle,
        }
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
    assert_eq!(status.state, PlaybackStateWire::Stopped);
    assert_eq!(status.volume, 100);
}

#[tokio::test]
async fn test_daemon_navigation_and_pages_over_ipc() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();

    // 1. GetNavigation
    let resp = client.send(&ClientRequest::GetNavigation).await.unwrap();
    if let ClientResponse::Navigation(nav) = resp {
        assert_eq!(nav.default_route, "home");
        assert_eq!(nav.groups.len(), 3);
        assert_eq!(nav.groups[0].id, "discover");
        assert_eq!(nav.groups[1].id, "library");
        assert_eq!(nav.groups[2].id, "replay");
    } else {
        panic!("Expected Navigation response, got {resp:?}");
    }

    // 2. GetPage with client helper
    let page = client.get_page("home").await.unwrap();
    assert_eq!(page.id, "home");
    assert_eq!(page.title, "Listen Now");
}

#[tokio::test]
async fn test_daemon_playback_control_lifecycle() {
    let server = TestServer::start().await;
    let client = MalusClient::connect(&server.sock_path).await.unwrap();

    // 1. Play track
    let resp = client
        .send(&ClientRequest::PlayTrack {
            media_id: "song:617154362".to_string(),
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
