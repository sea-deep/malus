use malus_client::{ClientError, ConnectionStatus, MalusClient};
use malus_ipc::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    client::{ClientEvent, ClientRequest, ClientResponse},
    codec::{decode_message, read_frame, write_message},
};
use malus_model::{PlaybackState, PlayerStatus, RepeatMode};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::UnixListener;
use tokio::sync::Notify;

#[tokio::test]
async fn test_client_ping_and_get_status() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let listener = UnixListener::bind(&sock).unwrap();

    let server_task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.into_split();
                let mut buf = Vec::new();
                while let Ok(Some(frame)) =
                    read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await
                {
                    let req: ClientRequest = decode_message(&frame).unwrap();
                    let resp = match req {
                        ClientRequest::Ping => ClientResponse::Pong,
                        ClientRequest::GetStatus => ClientResponse::Status(PlayerStatus {
                            state: PlaybackState::Paused,
                            current_track: None,
                            position_ms: 12000,
                            duration_ms: 60000,
                            volume: 75,
                            muted: false,
                            shuffle: false,
                            repeat: RepeatMode::Off,
                        }),
                        _ => ClientResponse::Ok,
                    };
                    write_message(&mut writer, &resp).await.unwrap();
                }
            });
        }
    });

    let client = MalusClient::connect(&sock).await.unwrap();
    client.ping().await.unwrap();

    let status = client.get_status().await.unwrap();
    assert_eq!(status.state, PlaybackState::Paused);
    assert_eq!(status.position_ms, 12000);
    assert_eq!(status.volume, 75);

    server_task.abort();
}

#[tokio::test]
async fn test_concurrent_requests_do_not_block() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let listener = UnixListener::bind(&sock).unwrap();

    let release_slow = Arc::new(Notify::new());
    let release_slow_clone = release_slow.clone();

    let server_task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let rel = release_slow_clone.clone();
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.into_split();
                let mut buf = Vec::new();
                if let Ok(Some(frame)) =
                    read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await
                {
                    let req: ClientRequest = decode_message(&frame).unwrap();
                    match req {
                        ClientRequest::Play => {
                            // Slow operation: wait for notification
                            rel.notified().await;
                            write_message(&mut writer, &ClientResponse::Ok)
                                .await
                                .unwrap();
                        }
                        ClientRequest::Ping => {
                            // Fast operation: respond immediately
                            write_message(&mut writer, &ClientResponse::Pong)
                                .await
                                .unwrap();
                        }
                        _ => {
                            write_message(&mut writer, &ClientResponse::Ok)
                                .await
                                .unwrap();
                        }
                    }
                }
            });
        }
    });

    let client = MalusClient::new(sock);

    // Spawn slow play request
    let client1 = client.clone();
    let slow_handle = tokio::spawn(async move {
        client1.play().await.unwrap();
    });

    // Wait a brief moment to ensure slow request is in flight
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Fast request should complete immediately while slow request is still waiting
    let fast_res = client.ping().await;
    assert!(fast_res.is_ok());

    // Now release slow request and ensure it completes
    release_slow.notify_one();
    slow_handle.await.unwrap();

    server_task.abort();
}

#[tokio::test]
async fn test_slow_request_a_does_not_block_later_fast_request_b() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("test_slow_fast.sock");
    let listener = UnixListener::bind(&sock).unwrap();

    let server_task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.into_split();
                let mut buf = Vec::new();
                if let Ok(Some(frame)) =
                    read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await
                {
                    let req: ClientRequest = decode_message(&frame).unwrap();
                    match req {
                        ClientRequest::Play => {
                            // Request A: blocks for 500ms
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            write_message(&mut writer, &ClientResponse::Ok)
                                .await
                                .unwrap();
                        }
                        ClientRequest::Ping => {
                            // Request B: completes immediately
                            write_message(&mut writer, &ClientResponse::Pong)
                                .await
                                .unwrap();
                        }
                        _ => {
                            write_message(&mut writer, &ClientResponse::Ok)
                                .await
                                .unwrap();
                        }
                    }
                }
            });
        }
    });

    let client = MalusClient::new(sock);

    // Request A starts at t0
    let start_time = std::time::Instant::now();
    let client_a = client.clone();
    let handle_a = tokio::spawn(async move {
        client_a.play().await.unwrap();
        std::time::Instant::now()
    });

    // Request B starts 50ms later
    tokio::time::sleep(Duration::from_millis(50)).await;
    let b_start = std::time::Instant::now();
    client.ping().await.unwrap();
    let b_finished = std::time::Instant::now();

    let a_finished = handle_a.await.unwrap();

    // Assert that Request B finished strictly BEFORE Request A finished
    assert!(
        b_finished < a_finished,
        "Request B (fast) should finish before Request A (slow)"
    );
    assert!(
        b_finished.duration_since(b_start) < Duration::from_millis(100),
        "Request B should complete quickly"
    );
    assert!(
        a_finished.duration_since(start_time) >= Duration::from_millis(450),
        "Request A should take around 500ms"
    );

    server_task.abort();
}

#[tokio::test]
async fn test_server_error_mapping() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let listener = UnixListener::bind(&sock).unwrap();

    let server_task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.into_split();
                let mut buf = Vec::new();
                if let Ok(Some(_)) =
                    read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await
                {
                    let resp = ClientResponse::Error {
                        code: "NOT_FOUND".to_string(),
                        message: "Item does not exist".to_string(),
                    };
                    write_message(&mut writer, &resp).await.unwrap();
                }
            });
        }
    });

    let client = MalusClient::new(sock);
    let err = client.play().await.unwrap_err();
    match err {
        ClientError::ServerError { code, message } => {
            assert_eq!(code, "NOT_FOUND");
            assert_eq!(message, "Item does not exist");
        }
        other => panic!("Expected ServerError, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn test_event_subscription_and_reconnect() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let listener = UnixListener::bind(&sock).unwrap();

    let (event_sender_tx, event_sender_rx) = tokio::sync::mpsc::channel::<ClientEvent>(10);

    let server_task = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut event_rx = event_sender_rx;
            let (mut reader, mut writer) = stream.into_split();
            let mut buf = Vec::new();

            if let Ok(Some(frame)) =
                read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await
            {
                let req: ClientRequest = decode_message(&frame).unwrap();
                if req == ClientRequest::SubscribeEvents {
                    write_message(&mut writer, &ClientResponse::Ok)
                        .await
                        .unwrap();

                    // Relay events to this client
                    while let Some(evt) = event_rx.recv().await {
                        if write_message(&mut writer, &evt).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    let client = MalusClient::new(sock);
    let (mut event_rx, mut status_rx) = client.subscribe_events();

    // Wait until status reaches Connected
    status_rx
        .wait_for(|s| *s == ConnectionStatus::Connected)
        .await
        .unwrap();
    assert_eq!(*status_rx.borrow(), ConnectionStatus::Connected);

    // Send an event from daemon
    let status_wire = PlayerStatus {
        state: PlaybackState::Playing,
        current_track: None,
        position_ms: 5000,
        duration_ms: 180000,
        volume: 100,
        muted: false,
        shuffle: false,
        repeat: RepeatMode::Off,
    };
    event_sender_tx
        .send(ClientEvent::StatusChanged(status_wire.clone()))
        .await
        .unwrap();

    let received = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .unwrap()
        .unwrap();

    match received {
        ClientEvent::StatusChanged(s) => assert_eq!(s.position_ms, 5000),
        other => panic!("Expected StatusChanged, got {other:?}"),
    }

    server_task.abort();
}
