//! `malus-provider-sdk`: Convenience SDK for Malus Audio Providers.
//!
//! # Architecture Boundary & Role
//!
//! The Rust `Provider` trait and this SDK are **strictly a development convenience**
//! for provider authors writing in Rust.
//!
//! The daemon (`malus-daemon`) communicates with providers **exclusively through the
//! language-independent wire protocol** over standard input/output with LSP-style
//! Content-Length framing.
//!
//! ```text
//! [ Rust Provider Trait ]
//!           ↓
//! [ malus_provider_sdk::serve(...) ]
//!           ↓
//! [ malus-protocol LSP frames on stdin/stdout ] <==== Wire Boundary ====> [ malus-daemon ]
//! ```
//!
//! Providers written in Python, Go, C++, Shell, or any other language implementing
//! the protocol manually over stdin/stdout are first-class peers and are completely
//! indistinguishable to `malus-daemon` from a Rust provider using this SDK.
//!
//! The daemon **never** depends on or invokes `malus-provider-sdk`.

pub mod error;
pub mod server;
pub mod traits;

pub use error::ProviderError;
pub use malus_protocol::{
    AlbumRefWire, AlbumWire, ArtistRefWire, ArtistWire, CatalogItemWire, LibraryKindWire,
    LibraryPageWire, MediaIdWire, PageWire, PlaybackStateWire, PlayerStatusWire, PlaylistWire,
    QueueWire, RepeatModeWire, SearchKindWire, SearchResultsWire, TrackWire,
};
pub use server::{serve, serve_io};
pub use traits::{Provider, capability};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use malus_protocol::{
        DEFAULT_MAX_PAYLOAD_BYTES, PlaybackStateWire, PlayerStatusWire, QueueWire, RepeatModeWire,
        TrackWire,
        codec::{decode_frame, decode_message, encode_message},
        provider::{ProviderRequest, ProviderResponse},
    };
    use std::sync::Arc;
    use tokio::io::duplex;

    struct DummyProvider;

    #[async_trait]
    impl Provider for DummyProvider {
        fn id(&self) -> &str {
            "dummy"
        }
        fn name(&self) -> &str {
            "Dummy"
        }
        fn capabilities(&self) -> Vec<String> {
            vec!["search".into()]
        }
        async fn search(
            &self,
            _query: &str,
            _kinds: &[malus_protocol::SearchKindWire],
            _limit: usize,
            _cursor: Option<&str>,
        ) -> Result<malus_protocol::SearchResultsWire, ProviderError> {
            Ok(malus_protocol::SearchResultsWire {
                tracks: Some(malus_protocol::PageWire::new(
                    vec![TrackWire::new("dummy:track:1", "Dummy Track", "Artist")],
                    None,
                )),
                albums: None,
                artists: None,
                playlists: None,
            })
        }
        async fn play(&self, _media_id: &str) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn pause(&self) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn resume(&self) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn stop(&self) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn next(&self) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn previous(&self) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn seek(&self, _position_ms: u64) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn set_volume(&self, _volume: u8) -> Result<(), ProviderError> {
            Ok(())
        }
        async fn get_status(&self) -> Result<PlayerStatusWire, ProviderError> {
            Ok(PlayerStatusWire {
                state: PlaybackStateWire::Stopped,
                current_track: None,
                position_ms: 0,
                duration_ms: 0,
                volume: 100,
                muted: false,
                shuffle: false,
                repeat: RepeatModeWire::Off,
            })
        }
        async fn get_queue(&self) -> Result<QueueWire, ProviderError> {
            Ok(QueueWire {
                items: Vec::new(),
                current_index: None,
            })
        }
        async fn enqueue(&self, _track: TrackWire) -> Result<(), ProviderError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_serve_io_dispatch() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (client_io, server_io) = duplex(4096);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (mut client_read, mut client_write) = tokio::io::split(client_io);

        let provider = Arc::new(DummyProvider);
        let srv_handle = tokio::spawn(async move {
            let _ = serve_io(provider, server_read, server_write).await;
        });

        // Send Ping in envelope
        let env = malus_protocol::provider::ProviderRequestEnvelope::new(1, ProviderRequest::Ping);
        let req_bytes = encode_message(&env).unwrap();
        client_write.write_all(&req_bytes).await.unwrap();
        client_write.flush().await.unwrap();

        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        let n = client_read.read(&mut chunk).await.unwrap();
        buf.extend_from_slice(&chunk[..n]);

        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let msg: malus_protocol::provider::ProviderWireMessage = decode_message(&frame).unwrap();
        assert_eq!(
            msg,
            malus_protocol::provider::ProviderWireMessage::response(1, ProviderResponse::Pong)
        );

        drop(client_write);
        drop(client_read);
        let _ = srv_handle.await;
    }

    #[tokio::test]
    async fn test_serve_io_event_emission() {
        use malus_protocol::codec::read_frame;
        use malus_protocol::provider::ProviderEvent;
        use tokio::io::AsyncWriteExt;
        use tokio::sync::Mutex;

        struct EventProvider {
            sink: Mutex<Option<tokio::sync::mpsc::UnboundedSender<ProviderEvent>>>,
        }

        #[async_trait::async_trait]
        impl Provider for EventProvider {
            fn id(&self) -> &str {
                "event-dummy"
            }
            fn name(&self) -> &str {
                "Event Dummy"
            }
            fn capabilities(&self) -> Vec<String> {
                vec![]
            }
            fn register_event_sink(&self, sink: tokio::sync::mpsc::UnboundedSender<ProviderEvent>) {
                *self.sink.try_lock().unwrap() = Some(sink);
            }
        }

        let (client_io, server_io) = duplex(4096);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (mut client_read, mut client_write) = tokio::io::split(client_io);

        let provider = Arc::new(EventProvider {
            sink: Mutex::new(None),
        });
        let prov_clone = provider.clone();
        let srv_handle = tokio::spawn(async move {
            let _ = serve_io(prov_clone, server_read, server_write).await;
        });

        // Give server a tick to register sink
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Provider emits an event
        let status = PlayerStatusWire {
            state: PlaybackStateWire::Playing,
            current_track: None,
            position_ms: 500,
            duration_ms: 1000,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        };
        let sink = provider.sink.lock().await.clone().unwrap();
        sink.send(ProviderEvent::StatusChanged(status.clone()))
            .unwrap();

        // Also client sends Ping request
        let env = malus_protocol::provider::ProviderRequestEnvelope::new(42, ProviderRequest::Ping);
        let req_bytes = encode_message(&env).unwrap();
        client_write.write_all(&req_bytes).await.unwrap();
        client_write.flush().await.unwrap();

        // Client reads frames: both event and response arrive
        let mut buf = Vec::new();
        let frame1 = read_frame(&mut client_read, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .await
            .unwrap()
            .unwrap();
        let msg1: malus_protocol::provider::ProviderWireMessage = decode_message(&frame1).unwrap();

        let frame2 = read_frame(&mut client_read, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .await
            .unwrap()
            .unwrap();
        let msg2: malus_protocol::provider::ProviderWireMessage = decode_message(&frame2).unwrap();

        let msgs = [msg1, msg2];
        let has_event = msgs.iter().any(|m| match m {
            malus_protocol::provider::ProviderWireMessage::Event { event } => {
                matches!(event, ProviderEvent::StatusChanged(_))
            }
            _ => false,
        });
        let has_resp = msgs.iter().any(|m| match m {
            malus_protocol::provider::ProviderWireMessage::Response { id, response } => {
                *id == 42 && matches!(response, ProviderResponse::Pong)
            }
            _ => false,
        });

        assert!(has_event, "Expected StatusChanged event in stream");
        assert!(has_resp, "Expected Pong response in stream");

        drop(client_write);
        drop(client_read);
        let _ = srv_handle.await;
    }
}
