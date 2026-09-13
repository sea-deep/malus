//! M5.1 Library Persistent Mutation Probe
//!
//! Gated live test verifying permanent additions to the Apple Music library:
//! - Song addition (POST /v1/me/library?ids[songs]=...)
//! - Album addition (POST /v1/me/library?ids[albums]=...)
//! - Playlist addition (POST /v1/me/library?ids[playlists]=...)
//! - Explicit rejection of Artist and Station library additions
//!
//! Gated strictly behind `MALUS_LIVE_PERSISTENT_MUTATIONS=1` and `MALUS_LIVE=1`.
//!
//! Run with:
//!   MALUS_LIVE=1 MALUS_LIVE_PERSISTENT_MUTATIONS=1 cargo test -p malus-service --test m5_library_mutation_probe -- --ignored --nocapture

use std::{sync::Arc, time::Duration};

use malus_model::MediaRef;
use malus_service::{AppleService, AppleWebSession, ProductionAppleWebSession};
use malus_wpe::ProfileManager;

#[tokio::test]
#[ignore = "live Apple library persistent mutation probe requiring authenticated profile"]
async fn probe_apple_library_persistent_mutations() {
    if std::env::var("MALUS_LIVE").unwrap_or_default() != "1"
        || std::env::var("MALUS_LIVE_PERSISTENT_MUTATIONS").unwrap_or_default() != "1"
    {
        println!(
            "Skipping probe_apple_library_persistent_mutations because MALUS_LIVE!=1 or MALUS_LIVE_PERSISTENT_MUTATIONS!=1"
        );
        return;
    }

    println!("\n=== M5.1 Apple Library Persistent Mutation Probe ===\n");

    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    assert!(
        token_path.exists(),
        "Authentication tokens.json must exist in apple profile"
    );

    let session: Arc<dyn AppleWebSession> = Arc::new(ProductionAppleWebSession::new());
    let service = AppleService::with_session(session.clone());

    // Helper to wait for asynchronous 202 library addition reflection
    async fn wait_for_in_library(service: &AppleService, mref: &MediaRef) -> bool {
        for _ in 0..10 {
            if let Ok(state) = service.get_media_state(mref).await
                && state.in_library
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    // 1. Song addition
    let song_ref = MediaRef::Song("6804643315".to_string()); // "Aasa Kooda"
    println!("--- Adding Song to Library: {song_ref} ---");
    let state_song = service
        .add_to_library(&song_ref)
        .await
        .expect("add song to library");
    let in_lib = if state_song.in_library {
        true
    } else {
        wait_for_in_library(&service, &song_ref).await
    };
    println!(
        "  Song state: in_library={}, favorite={}, rating={:?}",
        in_lib, state_song.favorite, state_song.rating
    );
    assert!(in_lib, "Song must be in library after addition");

    // 2. Album addition
    let album_ref = MediaRef::Album("1751425577".to_string()); // "Aasa Kooda - Single"
    println!("--- Adding Album to Library: {album_ref} ---");
    let state_album = service
        .add_to_library(&album_ref)
        .await
        .expect("add album to library");
    let in_lib_album = if state_album.in_library {
        true
    } else {
        wait_for_in_library(&service, &album_ref).await
    };
    println!(
        "  Album state: in_library={}, favorite={}, rating={:?}",
        in_lib_album, state_album.favorite, state_album.rating
    );
    assert!(in_lib_album, "Album must be in library after addition");

    // 3. Playlist addition
    let playlist_ref = MediaRef::Playlist("pl.276f415b1479409d8623fc5f348df4d2".to_string()); // "Viral Tamil"
    println!("--- Adding Playlist to Library: {playlist_ref} ---");
    let state_playlist = service
        .add_to_library(&playlist_ref)
        .await
        .expect("add playlist to library");
    let in_lib_playlist = if state_playlist.in_library {
        true
    } else {
        wait_for_in_library(&service, &playlist_ref).await
    };
    println!(
        "  Playlist state: in_library={}, favorite={}, rating={:?}",
        in_lib_playlist, state_playlist.favorite, state_playlist.rating
    );
    assert!(
        in_lib_playlist,
        "Playlist must be in library after addition"
    );

    // 4. Verify rejection of Artist and Station
    let artist_ref = MediaRef::Artist("178859458".to_string());
    println!("--- Verifying rejection of Artist library addition ---");
    let res_artist = service.add_to_library(&artist_ref).await;
    assert!(
        res_artist.is_err(),
        "Artist library addition must be rejected"
    );

    let station_ref = MediaRef::Station("ra.985484166".to_string());
    println!("--- Verifying rejection of Station library addition ---");
    let res_station = service.add_to_library(&station_ref).await;
    assert!(
        res_station.is_err(),
        "Station library addition must be rejected"
    );

    println!("\n=== ALL M5.1 LIBRARY PERSISTENT MUTATION PROBES PASSED! ===\n");
}
