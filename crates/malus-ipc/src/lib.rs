//! `malus-ipc`: Framing, wire representations, and client RPC definitions:
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
        let req = ClientRequest::PlayMedia {
            reference: MediaRef::parse("song:123").unwrap(),
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
        let res = ClientResponse::Status(PlayerStatus {
            state: PlaybackState::Playing,
            current_track: Some(Track::new(
                MediaRef::parse("song:1").unwrap(),
                "Song",
                "Artist",
            )),
            position_ms: 10_000,
            duration_ms: 200_000,
            volume: 80,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
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
        let auth_req = client::ClientRequest::GetAuthStatus;
        let encoded = encode_message(&auth_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(auth_req, decoded);

        let auth_res = client::ClientResponse::AuthStatus(AuthStatusWire::with_message(
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
            tracks: Some(PagedListWire::new(
                vec![Track::new(
                    MediaRef::parse("song:1").unwrap(),
                    "One More Time",
                    "Daft Punk",
                )],
                Some("cursor-2".to_string()),
            )),
            albums: Some(PagedListWire::new(
                vec![Album::new(MediaRef::parse("album:1").unwrap(), "Discovery")],
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
            reference: MediaRef::parse("album:697194953").unwrap(),
        };
        let encoded = encode_message(&cat_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(cat_req, decoded);

        let cat_res = client::ClientResponse::CatalogItem(CatalogItemWire::Album(Album::new(
            MediaRef::parse("album:697194953").unwrap(),
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
            reference: MediaRef::parse("album:697194953").unwrap(),
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

        let coll_res = client::ClientResponse::CollectionItems(PagedListWire::new(
            vec![Track::new(
                MediaRef::parse("song:1").unwrap(),
                "Track 1",
                "Artist 1",
            )],
            None,
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

        let lib_res =
            client::ClientResponse::LibraryPage(LibraryPageWire::Albums(PagedListWire::new(
                vec![Album::new(
                    MediaRef::parse("album:l.1").unwrap(),
                    "Saved Album",
                )],
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
    fn test_page_rpc_roundtrips() {
        // 1. Navigation request and response
        let nav_req = client::ClientRequest::GetNavigation;
        let encoded = encode_message(&nav_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(nav_req, decoded);

        let nav = NavigationWire::new(
            PageRoute::Home,
            vec![NavGroupWire::new(
                "main",
                Some("Main".to_string()),
                vec![NavEntryWire::with_icon(PageRoute::Home, "Home", "house")],
            )],
        );
        let nav_res = client::ClientResponse::Navigation(nav.clone());
        let encoded = encode_message(&nav_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(nav_res, decoded);

        // 2. Get page request and response
        let page_req = client::ClientRequest::GetPage {
            route: PageRoute::Home,
        };
        let encoded = encode_message(&page_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(page_req, decoded);

        let page = PageWire::new("home", "Home");
        let page_res = client::ClientResponse::Page(page.clone());
        let encoded = encode_message(&page_res).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientResponse = decode_message(&frame).unwrap();
        assert_eq!(page_res, decoded);

        // 3. Continue page request and response
        let cont_req = client::ClientRequest::ContinuePage {
            route: PageRoute::Home,
            cursor: PageCursorWire::section("sec-1", "cursor-xyz"),
        };
        let encoded = encode_message(&cont_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(cont_req, decoded);

        let cont_res = client::ClientResponse::PageContinued(PageContinuationWire::Section {
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

        // 4. Invoke action request and response
        let act_req = client::ClientRequest::InvokeAction {
            action: PageActionWire::Play(MediaRef::parse("song:123").unwrap()),
        };
        let encoded = encode_message(&act_req).unwrap();
        let mut buf = encoded;
        let frame = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let decoded: client::ClientRequest = decode_message(&frame).unwrap();
        assert_eq!(act_req, decoded);

        let act_res = client::ClientResponse::ActionResult(
            ActionResultWire::success().with_refresh(PageRefreshWire::CurrentPage),
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
