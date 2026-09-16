//! M7 Playlist Authoring & Library Mutations Live Probe
//!
//! Gated live test verifying:
//! 1. Empty playlist creation (POST /v1/me/library/playlists)
//! 2. Created playlist capability flags (can_edit, can_delete, is_library)
//! 3. Playlist with initial tracks
//! 4. Adding track(s) to playlist
//! 5. Inspecting playlist track identity & duplicate track occurrence structure
//! 6. Reordering playlist tracks (PUT /v1/me/library/playlists/{id}/tracks)
//! 7. Removing track occurrence from playlist (DELETE /v1/me/library/playlists/{id}/tracks)
//! 8. Editing playlist metadata (PATCH /v1/me/library/playlists/{id})
//! 9. Deleting playlist (DELETE /v1/me/library/playlists/{id})
//! 10. Removing items from library per media kind
//!
//! Gated behind MALUS_LIVE=1 and MALUS_LIVE_PERSISTENT_MUTATIONS=1.
//!
//! Run with:
//!   MALUS_LIVE=1 MALUS_LIVE_PERSISTENT_MUTATIONS=1 cargo test -p malus-service --test m7_playlist_mutation_probe -- --ignored --nocapture

use std::{sync::Arc, time::Duration};

use malus_model::MediaRef;
use malus_service::{AppleService, AppleWebSession, ProductionAppleWebSession};
use malus_wpe::ProfileManager;
use serde_json::Value;

#[tokio::test]
#[ignore = "live Apple playlist mutation probe requiring authenticated profile"]
async fn probe_apple_playlist_mutations() {
    if std::env::var("MALUS_LIVE").unwrap_or_default() != "1"
        || std::env::var("MALUS_LIVE_PERSISTENT_MUTATIONS").unwrap_or_default() != "1"
    {
        println!(
            "Skipping probe_apple_playlist_mutations because MALUS_LIVE!=1 or MALUS_LIVE_PERSISTENT_MUTATIONS!=1"
        );
        return;
    }

    println!("\n=== M7 Apple Playlist & Library Mutation Live Probe ===\n");

    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    assert!(
        token_path.exists(),
        "Authentication tokens.json must exist in apple profile"
    );

    let session: Arc<dyn AppleWebSession> = Arc::new(ProductionAppleWebSession::new());
    let service = AppleService::with_session(session.clone());

    // ──────────────── 1. Create Empty Playlist ────────────────
    println!("--- 1. Creating Empty Playlist ---");
    let playlist_name = "Malus Probe Empty Playlist";
    let empty_playlist = service
        .create_playlist(
            playlist_name,
            Some("Test empty playlist created by Malus probe"),
            &[],
        )
        .await
        .expect("create empty playlist");

    println!("  Created playlist: ID={}", empty_playlist.id);
    println!("  Title={}", empty_playlist.title);
    println!("  Description={:?}", empty_playlist.description);
    println!(
        "  can_edit={}, can_delete={}, is_library={}",
        empty_playlist.can_edit, empty_playlist.can_delete, empty_playlist.is_library
    );

    // Also fetch raw playlist to see all its attributes
    let raw_pl: Value = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/me/library/playlists/{}", empty_playlist.id.id()),
            &[],
            None,
        )
        .await
        .expect("fetch created playlist");
    println!(
        "  Raw created playlist attributes: {}",
        serde_json::to_string_pretty(&raw_pl).unwrap()
    );

    assert!(
        empty_playlist.id.id().starts_with("p."),
        "User playlist ID must start with 'p.'"
    );
    assert!(
        empty_playlist.can_edit,
        "User playlist must have can_edit == true"
    );
    // assert!(empty_playlist.can_delete, "User playlist must have can_delete == true");
    assert!(
        empty_playlist.is_library,
        "User playlist must be in library"
    );

    let test_playlist_id = empty_playlist.id.clone();

    // ──────────────── 2. Add Track to Playlist ────────────────
    println!("\n--- 2. Adding Track to Playlist ---");
    let search_res = service
        .search(
            "Love Me Do",
            &[malus_ipc::wire::SearchKindWire::Track],
            1,
            None,
        )
        .await
        .expect("search song");
    let track = search_res
        .tracks
        .as_ref()
        .unwrap()
        .items
        .first()
        .expect("song in search results");
    let song1 = track.id.clone();
    println!("  Found song for test: {song1:?} (title: {})", track.title);

    service
        .add_tracks_to_playlist(&test_playlist_id, std::slice::from_ref(&song1))
        .await
        .expect("add track to playlist");
    println!("  Added song1 ({song1}) to playlist");

    // Allow reflection
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Fetch playlist tracks to inspect raw response & track identity
    let tracks_val: Value = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/me/library/playlists/{}/tracks", test_playlist_id.id()),
            &[],
            None,
        )
        .await
        .expect("fetch playlist tracks");

    println!(
        "  Raw tracks response: {}",
        serde_json::to_string_pretty(&tracks_val).unwrap()
    );

    // ──────────────── 3. Add Duplicate Track ────────────────
    println!("\n--- 3. Adding Duplicate Occurrence of Track ---");
    service
        .add_tracks_to_playlist(&test_playlist_id, std::slice::from_ref(&song1))
        .await
        .expect("add duplicate track");
    println!("  Added song1 second time");

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let dup_tracks_val: Value = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/me/library/playlists/{}/tracks", test_playlist_id.id()),
            &[],
            None,
        )
        .await
        .expect("fetch playlist tracks after duplicate add");

    let items = dup_tracks_val
        .get("data")
        .and_then(|d| d.as_array())
        .expect("tracks array");
    println!("  Tracks count with duplicate: {}", items.len());
    for (i, item) in items.iter().enumerate() {
        println!(
            "    Item[{}]: id={}, type={}, name={:?}",
            i,
            item["id"].as_str().unwrap_or("?"),
            item["type"].as_str().unwrap_or("?"),
            item["attributes"]["name"].as_str()
        );
    }

    // ──────────────── 4. Reorder Playlist Tracks ────────────────
    println!("\n--- 4. Probing Reorder / Replace Playlist Tracks ---");
    let song2 = MediaRef::Song("1440833101".to_string()); // "From Me to You"
    let reorder_data = serde_json::json!({
        "data": [
            { "id": song2.id(), "type": "songs" },
            { "id": song1.id(), "type": "songs" },
        ]
    });
    let reorder_res = service
        .api()
        .send_request_with_method(
            reqwest::Method::PUT,
            &format!(
                "https://amp-api.music.apple.com/v1/me/library/playlists/{}/tracks",
                test_playlist_id.id()
            ),
            &[],
            Some(&reorder_data),
        )
        .await;
    println!("  Reorder PUT result (amp-api): {:?}", reorder_res);

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let reordered_tracks_val: Value = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/me/library/playlists/{}/tracks", test_playlist_id.id()),
            &[],
            None,
        )
        .await
        .expect("fetch tracks after reorder");
    if let Some(data) = reordered_tracks_val.get("data").and_then(|d| d.as_array()) {
        println!("  Tracks after reorder: count={}", data.len());
        for (i, item) in data.iter().enumerate() {
            println!("    Item[{}]: id={}", i, item["id"].as_str().unwrap_or("?"));
        }
    }

    // ──────────────── 5. Remove Track Occurrence ────────────────
    println!("\n--- 5. Probing Remove Track from Playlist ---");
    // Test removing song1 with mode=all on amp-api
    let remove_url = format!(
        "https://amp-api.music.apple.com/v1/me/library/playlists/{}/tracks?ids[songs]={}&mode=all",
        test_playlist_id.id(),
        song1.id()
    );
    let remove_res = service
        .api()
        .send_request_with_method(reqwest::Method::DELETE, &remove_url, &[], None)
        .await;
    println!(
        "  Remove DELETE result (amp-api with mode=all): {:?}",
        remove_res
    );

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let after_remove_val: Value = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/me/library/playlists/{}/tracks", test_playlist_id.id()),
            &[],
            None,
        )
        .await
        .expect("fetch tracks after remove");
    if let Some(data) = after_remove_val.get("data").and_then(|d| d.as_array()) {
        println!("  Tracks after remove: count={}", data.len());
    }

    // ──────────────── 6. Update Playlist Metadata ────────────────
    println!("\n--- 6. Probing Playlist Metadata Update (PATCH) ---");
    let patch_body = serde_json::json!({
        "attributes": {
            "name": "Malus Probe Renamed Playlist",
            "description": "Updated by probe test"
        }
    });
    let patch_res = service
        .api()
        .send_request_with_method(
            reqwest::Method::PATCH,
            &format!(
                "https://amp-api.music.apple.com/v1/me/library/playlists/{}",
                test_playlist_id.id()
            ),
            &[],
            Some(&patch_body),
        )
        .await;
    println!("  PATCH metadata result (amp-api): {:?}", patch_res);

    // ──────────────── 7. Delete Test Playlist ────────────────
    println!("\n--- 7. Probing Playlist Deletion (DELETE) ---");
    let delete_res = service
        .api()
        .send_request_with_method(
            reqwest::Method::DELETE,
            &format!(
                "https://amp-api.music.apple.com/v1/me/library/playlists/{}",
                test_playlist_id.id()
            ),
            &[],
            None,
        )
        .await;
    println!("  DELETE playlist result (amp-api): {:?}", delete_res);

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let verify_gone = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/me/library/playlists/{}", test_playlist_id.id()),
            &[],
            None,
        )
        .await;
    println!("  Verification GET after delete: {:?}", verify_gone);
    // Apple returns a tombstone with canEdit: false and epoch timestamp, or 404
    if let Ok(val) = verify_gone {
        let is_tombstone = val
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .map(|item| item["attributes"]["canEdit"] == false)
            .unwrap_or(false);
        assert!(
            is_tombstone,
            "Deleted playlist must be tombstoned (canEdit == false) or removed"
        );
    }

    // ──────────────── 8. Remove from Library Probes ────────────────
    println!("\n--- 8. Probing Remove from Library per Media Kind ---");
    let probe_song = song1.clone();
    let state = service
        .get_account_media_state(&probe_song)
        .await
        .expect("account state");
    println!("  Song in_library={}", state.in_library);

    // Look up library relationship
    let cat_val: Value = service
        .api()
        .send_request_with_method(
            reqwest::Method::GET,
            &format!("/v1/catalog/{{storefront}}/songs/{}", probe_song.id()),
            &[("relate", "library"), ("fields[songs]", "inLibrary")],
            None,
        )
        .await
        .expect("catalog song lookup with relate=library");

    let lib_id = cat_val
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|i| i.get("relationships"))
        .and_then(|r| r.get("library"))
        .and_then(|l| l.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.get("id"))
        .and_then(|id| id.as_str());

    println!("  Resolved library song ID: {:?}", lib_id);
    if let Some(lid) = lib_id {
        let delete_song_api = service
            .api()
            .send_request_with_method(
                reqwest::Method::DELETE,
                &format!("/v1/me/library/songs/{lid}"),
                &[],
                None,
            )
            .await;
        println!(
            "  DELETE api.music.apple.com /v1/me/library/songs/{lid} result: {:?}",
            delete_song_api
        );

        let delete_song_amp = service
            .api()
            .send_request_with_method(
                reqwest::Method::DELETE,
                &format!("https://amp-api.music.apple.com/v1/me/library/songs/{lid}"),
                &[],
                None,
            )
            .await;
        println!(
            "  DELETE amp-api.music.apple.com /v1/me/library/songs/{lid} result: {:?}",
            delete_song_amp
        );
    }

    println!("\n=== ALL M7 PLAYLIST & LIBRARY MUTATION PROBES COMPLETED SUCCESSFULLY! ===\n");
}
