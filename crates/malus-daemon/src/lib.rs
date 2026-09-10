//! Malus background daemon library.
//!
//! Provides the engine, process-isolated provider communication, and IPC server
//! for coordinating audio playback, queue management, and audio provider catalog lookups.
//!
//! Strictly does NOT depend on `malus-provider-sdk`.

pub mod discovery;
pub mod engine;
pub mod provider_process;
pub mod server;

pub use discovery::{DiscoveredProvider, discover_providers};
pub use engine::Engine;
pub use provider_process::{
    ProviderProcess, ProviderProcessError, ProviderSupervisorState, SupervisorPolicy,
};
pub use server::{Server, ServerError, default_socket_path};

#[cfg(test)]
mod tests {
    use super::*;
    use malus_protocol::{
        DEFAULT_MAX_PAYLOAD_BYTES,
        client::{ClientRequest, ClientResponse},
        codec::{decode_message, read_frame, write_message},
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn test_daemon_engine_ping_and_empty_state() {
        let engine = Engine::new();

        // Ping
        let pong = engine.handle_request(ClientRequest::Ping).await;
        assert_eq!(pong, ClientResponse::Pong);

        // Status when no provider active
        let status = engine.handle_request(ClientRequest::GetStatus).await;
        if let ClientResponse::Status(s) = status {
            assert_eq!(s.state, malus_protocol::PlaybackStateWire::Stopped);
        } else {
            panic!("Expected Status response");
        }
    }

    #[tokio::test]
    async fn test_server_ipc_roundtrip_lsp_framing() {
        let sock_dir = std::env::temp_dir();
        let sock_path = sock_dir.join(format!("malus-daemon-test-{}.sock", std::process::id()));

        let engine = Arc::new(Engine::new());
        let server = Server::new(&sock_path, engine);
        let srv_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait a moment for server to bind
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let stream = tokio::net::UnixStream::connect(&sock_path)
            .await
            .expect("Failed to connect to test socket");
        let (mut reader, mut writer) = stream.into_split();

        // Test Ping over LSP-framed IPC
        write_message(&mut writer, &ClientRequest::Ping)
            .await
            .unwrap();

        let mut buf = Vec::new();
        let frame = read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .await
            .unwrap()
            .expect("Should receive response frame");

        let resp: ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(resp, ClientResponse::Pong);

        drop(writer);
        drop(reader);
        srv_handle.abort();
        let _ = tokio::fs::remove_file(&sock_path).await;
    }
}
