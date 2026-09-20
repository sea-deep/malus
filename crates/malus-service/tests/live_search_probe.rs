//! Live acceptance probe for official Apple Music search.
//! Run with: cargo test -p malus-service --test live_search_probe -- --ignored --nocapture

use malus_ipc::wire::SearchKindWire;
use malus_model::MediaRef;
use malus_service::{OfficialAppleMusicApi, ProductionAppleWebSession, ProfileTokenProvider};
use malus_wpe::ProfileManager;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires real authenticated Apple Music profile"]
async fn test_live_search_official_probe() {
    println!("\n=== LIVE APPLE MUSIC OFFICIAL SEARCH ACCEPTANCE PROBE ===\n");

    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    if !token_path.exists() {
        eprintln!(
            "No tokens.json found at {:?}, skipping live test",
            token_path
        );
        return;
    }

    let session = Arc::new(ProductionAppleWebSession::new());
    let token_provider = Arc::new(ProfileTokenProvider::new(session.clone()));
    let api = Arc::new(OfficialAppleMusicApi::new(token_provider));

    // Test 1: Search "Coldplay" (Artist primary match)
    println!("--- 1. Testing Search for 'Coldplay' ---");
    let res = api
        .search("Coldplay", &[], 25, None)
        .await
        .expect("Search Coldplay");

    println!("Top Results present: {}", res.top_results.is_some());
    let top_results = res
        .top_results
        .as_ref()
        .expect("top_results must be populated");
    assert!(!top_results.is_empty(), "top_results must not be empty");
    println!("Top Results count: {}", top_results.len());

    // Print first 6 items in top_results (matches 2x3 lockup card grid)
    for (i, item) in top_results.iter().take(6).enumerate() {
        println!(
            "  [{}] title={:<28} | subtitle={:<28} | entity={:?}",
            i,
            item.title,
            item.subtitle.as_deref().unwrap_or("none"),
            item.entity
        );
    }

    // Top match must be the Artist Coldplay
    assert!(
        matches!(top_results[0].entity, Some(MediaRef::Artist(_))),
        "First top result for 'Coldplay' must be an Artist, got: {:?}",
        top_results[0].entity
    );
    assert_eq!(top_results[0].title, "Coldplay");

    // Check categorized groupings
    let tracks = res.tracks.as_ref().expect("tracks");
    println!("Songs count: {}", tracks.items.len());
    assert!(!tracks.items.is_empty(), "Songs must not be empty");

    let artists = res.artists.as_ref().expect("artists");
    println!("Artists count: {}", artists.items.len());
    assert!(!artists.items.is_empty(), "Artists must not be empty");

    let albums = res.albums.as_ref().expect("albums");
    println!("Albums count: {}", albums.items.len());
    assert!(!albums.items.is_empty(), "Albums must not be empty");

    let playlists = res.playlists.as_ref().expect("playlists");
    println!("Playlists count: {}", playlists.items.len());
    assert!(!playlists.items.is_empty(), "Playlists must not be empty");

    // Test 2: Search "Cruel Summer" (Song primary match)
    println!("\n--- 2. Testing Search for 'Cruel Summer' ---");
    let res_song = api
        .search("Cruel Summer", &[], 25, None)
        .await
        .expect("Search Cruel Summer");

    let song_top = res_song.top_results.as_ref().expect("top_results");
    assert!(!song_top.is_empty(), "top_results must not be empty");
    println!(
        "Top result: {} ({:?})",
        song_top[0].title, song_top[0].entity
    );
    assert!(
        matches!(song_top[0].entity, Some(MediaRef::Song(_))),
        "First top result for 'Cruel Summer' must be a Song"
    );

    // Test 3: Search with Stations kind
    println!("\n--- 3. Testing Search for 'Radio' with Station kind ---");
    let res_station = api
        .search("Radio", &[SearchKindWire::Station], 10, None)
        .await
        .expect("Search Radio stations");

    if let Some(ref st) = res_station.stations {
        println!("Stations returned: {}", st.items.len());
        for s in st.items.iter().take(3) {
            println!("  Station: {} | entity={:?}", s.title, s.entity);
        }
    }

    println!("\n=== ALL OFFICIAL SEARCH LIVE PROBE CHECKS PASSED ===");
}
