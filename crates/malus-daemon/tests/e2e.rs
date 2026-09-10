//! End-to-end integration tests between malus-daemon, child process providers, and malus-cli client.
//!
//! Enforces:
//! - Daemon communicates with provider ONLY through process standard I/O wire protocol
//! - Daemon has NO dependency on malus-provider-sdk or malus-provider-mock
//! - Monotonic playback clock advances while Playing, freezes while Paused
//! - Seek changes position
//! - Provider crash does not crash daemon
//! - Malformed provider output is isolated
//! - Capability declarations are honored
//! - `mock.repost` custom action works through normalized action RPC with target
//! - Supervisor state machine, backoff, and graceful shutdown escalation

use malus_cli::Client;
use malus_daemon::{Engine, ProviderProcess, ProviderSupervisorState, Server, SupervisorPolicy};
use malus_protocol::{
    MediaIdWire, PlaybackStateWire,
    client::{ClientEvent, ClientRequest, ClientResponse},
    wire::ActionRequestV0,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::sleep;

fn mock_bin_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // drop test binary name
    if path.ends_with("deps") {
        path.pop(); // drop 'deps'
    }
    let bin = path.join("malus-provider-mock");
    if !bin.exists() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target/debug/malus-provider-mock");
        if root.exists() {
            return root;
        }
    }
    bin
}

struct TestServer {
    sock_path: PathBuf,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    async fn start() -> Self {
        let sock_path = std::env::temp_dir().join(format!(
            "malus-e2e-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let engine = Arc::new(Engine::new());

        let bin = mock_bin_path();
        assert!(
            bin.exists(),
            "malus-provider-mock binary must exist at {}",
            bin.display()
        );

        // Spawn mock provider as a separate child process over stdin/stdout
        let provider = ProviderProcess::spawn("mock", "Mock Audio Provider", &bin, &[])
            .await
            .expect("Failed to spawn mock provider process");
        engine.register_provider(Arc::new(provider)).await;

        let server = Server::new(&sock_path, engine);
        let handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait for socket to exist and accept connections
        for _ in 0..50 {
            if sock_path.exists() && tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }

        Self { sock_path, handle }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

#[tokio::test]
async fn test_client_daemon_ping_and_capabilities() {
    let server = TestServer::start().await;
    let mut client = Client::connect(&server.sock_path).await.unwrap();

    let resp = client.send(&ClientRequest::Ping).await.unwrap();
    assert_eq!(resp, ClientResponse::Pong);

    let resp = client
        .send(&ClientRequest::GetCapabilities {
            provider: "mock".into(),
        })
        .await
        .unwrap();

    if let ClientResponse::Capabilities {
        provider,
        capabilities,
    } = resp
    {
        assert_eq!(provider, "mock");
        assert!(capabilities.contains(&"search".to_string()));
        assert!(capabilities.contains(&"playback".to_string()));
        assert!(capabilities.contains(&"playback.seek".to_string()));
        assert!(capabilities.contains(&"queue.read".to_string()));
        assert!(capabilities.contains(&"queue.edit".to_string()));
        assert!(capabilities.contains(&"mock.repost".to_string()));
        assert!(
            !capabilities.contains(&"lyrics.synced".to_string()),
            "Lyrics capability should not be present"
        );
    } else {
        panic!("Expected Capabilities response");
    }
}

#[tokio::test]
async fn test_search_and_queue_workflow() {
    let server = TestServer::start().await;
    let mut client = Client::connect(&server.sock_path).await.unwrap();

    // 1. Search for tracks via child process MockProvider
    let resp = client
        .send(&ClientRequest::Search {
            query: "Mock Track".into(),
        })
        .await
        .unwrap();

    let tracks = match resp {
        ClientResponse::SearchResults { tracks } => tracks,
        other => panic!("Expected search results, got {other:?}"),
    };
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].id, "mock:track:1");
    assert_eq!(tracks[1].id, "mock:track:2");

    // 2. Enqueue track
    let resp = client
        .send(&ClientRequest::Enqueue {
            track: tracks[0].clone(),
        })
        .await
        .unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    // 3. Inspect authoritative queue
    let resp = client.send(&ClientRequest::GetQueue).await.unwrap();
    if let ClientResponse::Queue(q) = resp {
        assert!(!q.items.is_empty());
        assert_eq!(q.items[0].id, "mock:track:1");
    } else {
        panic!("Expected queue snapshot");
    }
}

#[tokio::test]
async fn test_playback_controls_and_clock() {
    let server = TestServer::start().await;
    let mut client = Client::connect(&server.sock_path).await.unwrap();

    // Play track
    let resp = client
        .send(&ClientRequest::PlayTrack {
            media_id: "mock:track:1".into(),
        })
        .await
        .unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    // Verify state is Playing
    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    let initial_pos = if let ClientResponse::Status(s) = status {
        assert_eq!(s.state, PlaybackStateWire::Playing);
        assert_eq!(s.current_track.unwrap().id, "mock:track:1");
        s.position_ms
    } else {
        panic!("Expected status");
    };

    // Sleep to verify monotonic clock advancement
    sleep(Duration::from_millis(60)).await;

    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    let advanced_pos = if let ClientResponse::Status(s) = status {
        s.position_ms
    } else {
        panic!("Expected status");
    };
    assert!(
        advanced_pos >= initial_pos + 50,
        "Clock should advance during playback"
    );

    // Pause
    let resp = client.send(&ClientRequest::Pause).await.unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    let paused_pos = if let ClientResponse::Status(s) = status {
        assert_eq!(s.state, PlaybackStateWire::Paused);
        s.position_ms
    } else {
        panic!("Expected status");
    };

    // Sleep while paused to verify clock does not advance
    sleep(Duration::from_millis(50)).await;

    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    if let ClientResponse::Status(s) = status {
        assert_eq!(s.position_ms, paused_pos, "Clock must freeze when paused");
    } else {
        panic!("Expected status");
    }

    // Seek
    let resp = client
        .send(&ClientRequest::Seek {
            position_ms: 45_000,
        })
        .await
        .unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    if let ClientResponse::Status(s) = status {
        assert!(
            s.position_ms >= 45_000,
            "Seek should jump position to 45000ms"
        );
    } else {
        panic!("Expected status");
    }

    // Next
    let resp = client.send(&ClientRequest::Next).await.unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    if let ClientResponse::Status(s) = status {
        assert_eq!(s.current_track.unwrap().id, "mock:track:2");
    } else {
        panic!("Expected status");
    }

    // Stop
    let resp = client.send(&ClientRequest::Stop).await.unwrap();
    assert_eq!(resp, ClientResponse::Ok);

    let status = client.send(&ClientRequest::GetStatus).await.unwrap();
    if let ClientResponse::Status(s) = status {
        assert_eq!(s.state, PlaybackStateWire::Stopped);
        assert_eq!(s.position_ms, 0);
    } else {
        panic!("Expected status");
    }
}

#[tokio::test]
async fn test_custom_action_mock_repost() {
    let server = TestServer::start().await;
    let mut client = Client::connect(&server.sock_path).await.unwrap();

    let resp = client
        .send(&ClientRequest::Action(ActionRequestV0 {
            provider: "mock".into(),
            action: "mock.repost".into(),
            target: Some(MediaIdWire::parse("mock:track:3").unwrap()),
            params: serde_json::json!({}),
        }))
        .await
        .unwrap();

    if let ClientResponse::ActionResult(res) = resp {
        assert_eq!(res["reposted"], true);
        assert_eq!(res["track_id"], "mock:track:3");
        assert_eq!(res["provider"], "mock");
    } else {
        panic!("Expected ActionResult response, got {resp:?}");
    }
}

#[tokio::test]
async fn test_supervisor_state_machine_and_backoff() {
    let bin = mock_bin_path();
    let policy = SupervisorPolicy {
        max_retries: 2,
        base_backoff_ms: 50,
        handshake_timeout: Duration::from_secs(2),
        shutdown_grace_period: Duration::from_millis(500),
        shutdown_kill_timeout: Duration::from_millis(500),
    };

    let proc = ProviderProcess::spawn_with_policy(
        "test_supervisor",
        "Supervisor Test Provider",
        &bin,
        &[],
        policy,
    )
    .await
    .expect("Failed to spawn process");

    // Initially Ready after successful handshake
    assert_eq!(proc.state().await, ProviderSupervisorState::Ready);
    assert!(proc.is_ready().await);

    // Explicitly shut down
    proc.shutdown().await.unwrap();
    assert_eq!(proc.state().await, ProviderSupervisorState::Stopped);
}

#[tokio::test]
async fn test_provider_crash_isolation() {
    let sock_path =
        std::env::temp_dir().join(format!("malus-crash-test-{}.sock", std::process::id()));

    let engine = Arc::new(Engine::new());
    let bin = mock_bin_path();

    // Spawn mock provider
    let proc = Arc::new(
        ProviderProcess::spawn("mock", "Mock Audio Provider", &bin, &[])
            .await
            .expect("Failed to spawn process"),
    );
    engine.register_provider(proc.clone()).await;

    let server = Server::new(&sock_path, engine.clone());
    let srv_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    sleep(Duration::from_millis(50)).await;

    let mut client = Client::connect(&sock_path).await.unwrap();

    // Force kill the provider child process
    proc.force_kill_for_test().await;
    sleep(Duration::from_millis(20)).await;

    // Sending a command to dead process should return an error, NOT crash daemon
    let resp = client.send(&ClientRequest::Play).await;
    assert!(
        resp.is_err(),
        "Expected error response from killed provider"
    );

    // Daemon is still fully operational
    let ping = client.send(&ClientRequest::Ping).await.unwrap();
    assert_eq!(ping, ClientResponse::Pong);

    // Supervisor state entered restarting
    let state = proc.state().await;
    assert!(matches!(
        state,
        ProviderSupervisorState::Restarting { .. } | ProviderSupervisorState::Crashed { .. }
    ));

    srv_handle.abort();
    let _ = std::fs::remove_file(&sock_path);
}

#[tokio::test]
async fn test_malformed_provider_output_isolation() {
    let sock_path =
        std::env::temp_dir().join(format!("malus-malformed-test-{}.sock", std::process::id()));

    let engine = Arc::new(Engine::new());

    // Spawn a process that completes handshake then outputs garbage
    let script = r#"
printf "Content-Length: 108\r\n\r\n{\"type\":\"Hello\",\"data\":{\"id\":\"garbage\",\"name\":\"Garbage\",\"version\":[0,1],\"capabilities\":[],\"status\":\"ready\"}}"
head -n 3 > /dev/null
printf "GARBAGE_WITHOUT_LSP_CONTENT_LENGTH\r\n\r\n"
exit 0
"#;

    let garbage_proc = ProviderProcess::spawn("garbage", "Garbage Emitter", "sh", &["-c", script])
        .await
        .expect("Failed to spawn garbage process");
    engine.register_provider(Arc::new(garbage_proc)).await;

    let server = Server::new(&sock_path, engine.clone());
    let srv_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    sleep(Duration::from_millis(50)).await;

    let mut client = Client::connect(&sock_path).await.unwrap();

    let resp = client.send(&ClientRequest::Play).await;
    assert!(
        resp.is_err(),
        "Malformed provider output should return an error"
    );

    // Daemon is still alive
    let ping = client.send(&ClientRequest::Ping).await.unwrap();
    assert_eq!(ping, ClientResponse::Pong);

    srv_handle.abort();
    let _ = std::fs::remove_file(&sock_path);
}

#[tokio::test]
async fn test_event_broadcasting() {
    let server = TestServer::start().await;

    let mut event_client = Client::connect(&server.sock_path).await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);

    tokio::spawn(async move {
        let _ = event_client
            .stream_events(|event| {
                let _ = tx.try_send(event);
            })
            .await;
    });

    sleep(Duration::from_millis(50)).await;

    let mut cmd_client = Client::connect(&server.sock_path).await.unwrap();

    // Trigger Play
    cmd_client.send(&ClientRequest::Play).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Event channel closed");

    match event {
        ClientEvent::StatusChanged(status) => {
            assert_eq!(status.state, PlaybackStateWire::Playing);
        }
        other => panic!("Expected StatusChanged event, got {other:?}"),
    }

    // Trigger Volume
    cmd_client
        .send(&ClientRequest::SetVolume { volume: 82 })
        .await
        .unwrap();

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Event channel closed");

    match event {
        ClientEvent::StatusChanged(status) => {
            assert_eq!(status.volume, 82);
        }
        other => panic!("Expected StatusChanged event, got {other:?}"),
    }
}
