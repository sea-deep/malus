//! `malus-protocol`: Framing, wire representations, and two distinct RPC trust boundaries:
//! - `client`: Frontend (CLI/TUI/GUI) <-> `malusd` IPC over Unix domain sockets
//! - `provider`: `malusd` <-> Provider child processes over standard I/O

pub mod client;
pub mod framing;
pub mod provider;
pub mod wire;

// Re-export common framing and wire types for convenient access
pub use framing::*;
pub use wire::*;

#[cfg(test)]
mod tests {
    use super::*;
    use client::{ClientRequest, ClientResponse};
    use provider::{ProviderRequest, ProviderResponse};

    #[test]
    fn test_client_request_roundtrip() {
        let req = ClientRequest::Enqueue {
            track: TrackWire::new("mock:track:123", "Test Title", "Test Artist"),
        };
        let encoded = encode_message(&req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_client_response_roundtrip() {
        let res = ClientResponse::Status(PlayerStatusWire {
            state: PlaybackStateWire::Playing,
            current_track: Some(TrackWire::new("mock:track:1", "Song", "Artist")),
            position_ms: 10_000,
            duration_ms: 200_000,
            volume: 80,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        });
        let encoded = encode_message(&res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(res, decoded);
    }

    #[test]
    fn test_provider_request_roundtrip() {
        let req = ProviderRequest::Action(ActionRequestV0 {
            provider: "mock".into(),
            action: "mock.repost".into(),
            target: Some(MediaIdWire::parse("mock:track:3").unwrap()),
            params: serde_json::json!({ "note": "cool song" }),
        });
        let encoded = encode_message(&req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: ProviderRequest = decode_message(&frame).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_provider_response_roundtrip() {
        let res = ProviderResponse::Capabilities(vec!["playback".into(), "mock.repost".into()]);
        let encoded = encode_message(&res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: ProviderResponse = decode_message(&frame).unwrap();
        assert_eq!(res, decoded);
    }

    #[test]
    fn test_protocol_versions() {
        assert_eq!(client::CLIENT_PROTOCOL, (0, 1));
        assert_eq!(provider::PROVIDER_PROTOCOL, (0, 1));
    }

    #[test]
    fn test_auth_request_and_response_roundtrip() {
        let auth_req = client::ClientRequest::GetAuthStatus {
            provider: "apple".to_string(),
        };
        let encoded = encode_message(&auth_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(auth_req, decoded);

        let auth_res = client::ClientResponse::AuthStatus(AuthStatusWire::with_message(
            "apple",
            AuthStateWire::NeedsAuth,
            "Sign-in required",
        ));
        let encoded_res = encode_message(&auth_res).unwrap();
        let mut buf_res = encoded_res;
        let frame_res = decode_frame(&mut buf_res, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded_res: client::ClientResponse = decode_message(&frame_res).unwrap();
        assert_eq!(auth_res, decoded_res);

        let prov_req = provider::ProviderRequest::AuthBegin;
        let prov_encoded = encode_message(&prov_req).unwrap();
        let mut prov_buf = prov_encoded;
        let prov_frame = decode_frame(&mut prov_buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let prov_decoded: provider::ProviderRequest = decode_message(&prov_frame).unwrap();
        assert_eq!(prov_req, prov_decoded);
    }
}
