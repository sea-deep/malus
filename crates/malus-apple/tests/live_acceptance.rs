//! Live acceptance test for Apple provider over native HTTP and WPE playback.
//!
//! Run with:
//! cargo test -p malus-provider-apple --test live_acceptance -- --ignored --nocapture

use std::{sync::Arc, time::Duration};

use malus_apple::{
    AppleCredentials, AppleProvider, AppleWebSession, OfficialAppleMusicApi,
    ProductionAppleWebSession, ProfileTokenProvider,
};
use malus_protocol::wire::{CatalogItemWire, LibraryKindWire, LibraryPageWire, SearchKindWire};
use malus_web_runtime::ProfileManager;

#[tokio::test]
#[ignore = "requires real authenticated Apple Music profile"]
async fn test_live_apple_provider_acceptance() {
    println!("\n=== Starting Apple Provider Live Acceptance Suite ===\n");

    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    assert!(
        token_path.exists(),
        "Authentication tokens.json must exist in apple profile"
    );

    let creds = AppleCredentials::load_from_file(&token_path).expect("cached tokens readable");
    println!("Storefront: {}", creds.storefront);

    // Instantiate provider with ProductionAppleWebSession and OfficialAppleMusicApi
    let session = Arc::new(ProductionAppleWebSession::new());
    let token_provider = Arc::new(ProfileTokenProvider::new(session.clone()));
    let api = Arc::new(OfficialAppleMusicApi::new(token_provider));
    let provider = AppleProvider::with_session_and_api(session.clone(), api);

    // 1. Verify Search ("Daft Punk")
    println!("\n--- 1. Testing Search ('Daft Punk') ---");
    let search_res = provider
        .search(
            "Daft Punk",
            &[
                SearchKindWire::Track,
                SearchKindWire::Album,
                SearchKindWire::Artist,
                SearchKindWire::Playlist,
            ],
            5,
            None,
        )
        .await
        .expect("search succeeded");

    let tracks = search_res.tracks.expect("tracks returned");
    assert!(!tracks.items.is_empty(), "tracks not empty");
    let first_track = &tracks.items[0];
    println!("  First track: {} - {}", first_track.id, first_track.title);

    let albums = search_res.albums.expect("albums returned");
    assert!(!albums.items.is_empty(), "albums not empty");
    let first_album = &albums.items[0];
    println!("  First album: {} - {}", first_album.id, first_album.title);

    let artists = search_res.artists.expect("artists returned");
    assert!(!artists.items.is_empty(), "artists not empty");
    let first_artist = &artists.items[0];
    println!(
        "  First artist: {} - {}",
        first_artist.id, first_artist.name
    );

    let playlists = search_res.playlists.expect("playlists returned");
    assert!(!playlists.items.is_empty(), "playlists not empty");
    let first_playlist = &playlists.items[0];
    println!(
        "  First playlist: {} - {}",
        first_playlist.id, first_playlist.title
    );

    // 2. Verify Catalog Lookups
    println!("\n--- 2. Testing Catalog Lookups ---");
    let track_item = provider
        .get_catalog_item(&first_track.id)
        .await
        .expect("catalog track lookup");
    match track_item {
        CatalogItemWire::Track(t) => {
            assert_eq!(t.id, first_track.id);
            println!("  Catalog track verified: {}", t.title);
        }
        _ => panic!("expected CatalogItemWire::Track"),
    }

    let album_item = provider
        .get_catalog_item(&first_album.id)
        .await
        .expect("catalog album lookup");
    match album_item {
        CatalogItemWire::Album(a) => {
            assert_eq!(a.id, first_album.id);
            println!("  Catalog album verified: {}", a.title);
        }
        _ => panic!("expected CatalogItemWire::Album"),
    }

    let artist_item = provider
        .get_catalog_item(&first_artist.id)
        .await
        .expect("catalog artist lookup");
    match artist_item {
        CatalogItemWire::Artist(a) => {
            assert_eq!(a.id, first_artist.id);
            println!("  Catalog artist verified: {}", a.name);
        }
        _ => panic!("expected CatalogItemWire::Artist"),
    }

    let playlist_item = provider
        .get_catalog_item(&first_playlist.id)
        .await
        .expect("catalog playlist lookup");
    match playlist_item {
        CatalogItemWire::Playlist(p) => {
            assert_eq!(p.id, first_playlist.id);
            println!("  Catalog playlist verified: {}", p.title);
        }
        _ => panic!("expected CatalogItemWire::Playlist"),
    }

    // 3. Verify Library Reads
    println!("\n--- 3. Testing Library Reads ---");
    let lib_tracks = provider
        .get_library(LibraryKindWire::Tracks, 5, None)
        .await
        .expect("library tracks");
    match lib_tracks {
        LibraryPageWire::Tracks(page) => {
            println!("  Library tracks count: {}", page.items.len());
        }
        _ => panic!("expected tracks page"),
    }

    let lib_albums = provider
        .get_library(LibraryKindWire::Albums, 5, None)
        .await
        .expect("library albums");
    match lib_albums {
        LibraryPageWire::Albums(page) => {
            println!("  Library albums count: {}", page.items.len());
        }
        _ => panic!("expected albums page"),
    }

    let lib_playlists = provider
        .get_library(LibraryKindWire::Playlists, 5, None)
        .await
        .expect("library playlists");
    match lib_playlists {
        LibraryPageWire::Playlists(page) => {
            println!("  Library playlists count: {}", page.items.len());
        }
        _ => panic!("expected playlists page"),
    }

    // 4. Verify Playback
    println!("\n--- 4. Testing Playback (WPE Engine) ---");
    println!("  Playing track {}...", first_track.id);
    provider.play(&first_track.id).await.expect("play track");

    // Wait up to 15s for playback status to become Playing
    let mut playing = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let status = provider.get_status().await.expect("get status");
        println!(
            "  Current status: state={:?}, pos={:?}, dur={:?}",
            status.state, status.position_ms, status.duration_ms
        );
        if status.state == malus_protocol::PlaybackStateWire::Playing {
            playing = true;
            break;
        }
    }
    assert!(playing, "Playback did not enter Playing state in time");

    // Pause
    println!("  Pausing...");
    provider.pause().await.expect("pause");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = provider.get_status().await.expect("get status");
    println!("  Status after pause: state={:?}", status.state);
    assert_eq!(status.state, malus_protocol::PlaybackStateWire::Paused);

    // Resume
    println!("  Resuming...");
    provider.resume().await.expect("resume");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = provider.get_status().await.expect("get status");
    println!("  Status after resume: state={:?}", status.state);
    assert_eq!(status.state, malus_protocol::PlaybackStateWire::Playing);

    // Seek
    println!("  Seeking to 15s...");
    provider.seek(15_000).await.expect("seek");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = provider.get_status().await.expect("get status");
    println!("  Status after seek: pos={:?}", status.position_ms);

    // Stop
    println!("  Stopping...");
    provider.stop().await.expect("stop");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = provider.get_status().await.expect("get status");
    println!("  Status after stop: state={:?}", status.state);
    assert_eq!(status.state, malus_protocol::PlaybackStateWire::Stopped);

    // Shutdown session
    let _ = session.shutdown().await;

    println!("\n=== Live Acceptance Suite Passed Successfully ===\n");
}
