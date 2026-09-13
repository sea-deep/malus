//! `malus-protocol`: Framing, wire representations, and client RPC definitions:
//! - `client`: Frontend (CLI/TUI/GUI) <-> `malusd` IPC over Unix domain sockets

pub mod client;
pub mod framing;
pub mod wire;

// Re-export common framing and wire types for convenient access
pub use framing::*;
pub use wire::*;

#[cfg(test)]
mod tests {
    use super::*;
    use client::{ClientRequest, ClientResponse};

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
    fn test_protocol_versions() {
        assert_eq!(client::CLIENT_PROTOCOL, (0, 1));
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
    }

    #[test]
    fn test_m1d_rpc_roundtrips() {
        // 1. Client Search request & response
        let search_req = client::ClientRequest::Search {
            query: "daft punk".to_string(),
            kinds: vec![SearchKindWire::Album, SearchKindWire::Track],
            provider: Some("apple".to_string()),
            limit: Some(15),
            cursor: Some("token-1".to_string()),
        };
        let encoded = encode_message(&search_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(search_req, decoded);

        let search_res = client::ClientResponse::SearchResults(SearchResultsWire {
            tracks: Some(PageWire::new(
                vec![TrackWire::new(
                    "apple:track:1",
                    "One More Time",
                    "Daft Punk",
                )],
                Some("cursor-2".to_string()),
            )),
            albums: Some(PageWire::new(
                vec![AlbumWire::new("apple:album:1", "Discovery")],
                None,
            )),
            artists: None,
            playlists: None,
        });
        let encoded = encode_message(&search_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(search_res, decoded);

        // 2. Catalog item request & response
        let cat_req = client::ClientRequest::GetCatalogItem {
            media_id: "apple:album:697194953".to_string(),
        };
        let encoded = encode_message(&cat_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(cat_req, decoded);

        let cat_res = client::ClientResponse::CatalogItem(CatalogItemWire::Album(AlbumWire::new(
            "apple:album:697194953",
            "Discovery",
        )));
        let encoded = encode_message(&cat_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(cat_res, decoded);

        // 3. Collection items request & response
        let coll_req = client::ClientRequest::GetCollectionItems {
            media_id: "apple:album:697194953".to_string(),
            limit: Some(25),
            cursor: None,
        };
        let encoded = encode_message(&coll_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(coll_req, decoded);

        let coll_res = client::ClientResponse::CollectionItems(PageWire::new(
            vec![TrackWire::new("apple:track:1", "Track 1", "Artist 1")],
            Some("next-cursor".to_string()),
        ));
        let encoded = encode_message(&coll_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(coll_res, decoded);

        // 4. Library request & response
        let lib_req = client::ClientRequest::GetLibrary {
            kind: LibraryKindWire::Albums,
            provider: Some("apple".to_string()),
            limit: Some(50),
            cursor: Some("lib-cursor-1".to_string()),
        };
        let encoded = encode_message(&lib_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(lib_req, decoded);

        let lib_res = client::ClientResponse::LibraryPage(LibraryPageWire::Albums(PageWire::new(
            vec![AlbumWire::new("apple:album:l.1", "Saved Album")],
            None,
        )));
        let encoded = encode_message(&lib_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(lib_res, decoded);
    }

    #[test]
    fn test_surface_rpc_roundtrips() {
        // 1. Client manifest request and response
        let manifest_req = client::ClientRequest::GetProviderSurfaceManifest {
            provider: "mock".to_string(),
        };
        let encoded = encode_message(&manifest_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(manifest_req, decoded);

        let manifest = AppleNavigationWire::new(
            "home",
            vec![NavGroupWire::new(
                "main",
                Some("Main".to_string()),
                vec![NavEntryWire::with_icon("home", "Home", "house")],
            )],
        );
        let manifest_res = client::ClientResponse::ProviderSurfaceManifest(manifest.clone());
        let encoded = encode_message(&manifest_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(manifest_res, decoded);

        // 2. Client get surface request and response
        let surface_req = client::ClientRequest::GetSurface {
            provider: "mock".to_string(),
            surface_id: "home".to_string(),
        };
        let encoded = encode_message(&surface_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(surface_req, decoded);

        let surface = ApplePageWire::new("home", "Home");
        let surface_res = client::ClientResponse::Surface(surface.clone());
        let encoded = encode_message(&surface_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(surface_res, decoded);

        // 3. Client continue surface
        let cont_req = client::ClientRequest::ContinueSurface {
            provider: "mock".to_string(),
            surface_id: "home".to_string(),
            cursor: PageCursorWire::section("sec-1", "cursor-xyz"),
        };
        let encoded = encode_message(&cont_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(cont_req, decoded);

        let cont_res = client::ClientResponse::SurfaceContinued(PageContinuationWire::Section {
            section_id: "sec-1".to_string(),
            items: vec![PageItemWire::new("item-next", "Next Item")],
            continuation: None,
        });
        let encoded = encode_message(&cont_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(cont_res, decoded);

        // 4. Client invoke action
        let act_req = client::ClientRequest::InvokeSurfaceAction {
            provider: "mock".to_string(),
            invocation_token: "tok:test-action".to_string(),
        };
        let encoded = encode_message(&act_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(act_req, decoded);

        let act_res = client::ClientResponse::SurfaceActionResult(
            SurfaceActionResultWire::success().with_refresh(SurfaceRefreshWire::CurrentSurface),
        );
        let encoded = encode_message(&act_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(act_res, decoded);
    }
}
