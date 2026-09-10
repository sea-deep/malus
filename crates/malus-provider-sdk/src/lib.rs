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
    MediaIdWire, PlaybackStateWire, PlayerStatusWire, QueueWire, RepeatModeWire, TrackWire,
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
            _limit: usize,
        ) -> Result<Vec<TrackWire>, ProviderError> {
            Ok(vec![TrackWire::new(
                "dummy:track:1",
                "Dummy Track",
                "Artist",
            )])
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

        // Send Ping
        let req_bytes = encode_message(&ProviderRequest::Ping).unwrap();
        client_write.write_all(&req_bytes).await.unwrap();
        client_write.flush().await.unwrap();

        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        let n = client_read.read(&mut chunk).await.unwrap();
        buf.extend_from_slice(&chunk[..n]);

        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let resp: ProviderResponse = decode_message(&frame).unwrap();
        assert_eq!(resp, ProviderResponse::Pong);

        drop(client_write);
        drop(client_read);
        let _ = srv_handle.await;
    }
}
