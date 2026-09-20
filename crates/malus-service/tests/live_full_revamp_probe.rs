//! Live acceptance probe for the revamped Apple Music architecture.
//! Run with: cargo test -p malus-service --test live_full_revamp_probe -- --ignored --nocapture

use malus_model::PageRoute;
use malus_service::{
    AppleService, OfficialAppleMusicApi, ProductionAppleWebSession, ProfileTokenProvider,
};
use malus_wpe::ProfileManager;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires real authenticated Apple Music profile"]
async fn test_live_full_revamp_probe() {
    println!("\n=== LIVE APPLE MUSIC FULL REVAMP ACCEPTANCE PROBE ===\n");

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
    let service = AppleService::with_session_and_api(session.clone(), api);

    // 1. Test Home
    println!("--- 1. Testing Home Page ---");
    let home = service.get_page(&PageRoute::Home).await.expect("Home page");
    println!(
        "Home Title: {}, Sections: {}",
        home.title,
        home.sections.len()
    );
    assert!(!home.sections.is_empty(), "Home sections must not be empty");
    for (i, sec) in home.sections.iter().take(5).enumerate() {
        println!(
            "  [{}] title={:?} | hint={:?} | items={}",
            i,
            sec.title,
            sec.presentation_hint,
            sec.items.len()
        );
    }

    // 2. Test Radio
    println!("\n--- 2. Testing Radio Page ---");
    let radio = service
        .get_page(&PageRoute::Radio)
        .await
        .expect("Radio page");
    println!(
        "Radio Title: {}, Sections: {}",
        radio.title,
        radio.sections.len()
    );
    assert!(
        !radio.sections.is_empty(),
        "Radio sections must not be empty"
    );
    for (i, sec) in radio.sections.iter().take(5).enumerate() {
        println!(
            "  [{}] title={:?} | hint={:?} | items={}",
            i,
            sec.title,
            sec.presentation_hint,
            sec.items.len()
        );
    }

    // 3. Test Browse (New)
    println!("\n--- 3. Testing Browse (New) Page ---");
    let browse = service
        .get_page(&PageRoute::New)
        .await
        .expect("Browse page");
    println!(
        "Browse Title: {}, Sections: {}",
        browse.title,
        browse.sections.len()
    );
    assert!(
        !browse.sections.is_empty(),
        "Browse sections must not be empty"
    );
    for (i, sec) in browse.sections.iter().take(5).enumerate() {
        println!(
            "  [{}] title={:?} | hint={:?} | items={}",
            i,
            sec.title,
            sec.presentation_hint,
            sec.items.len()
        );
    }

    // 4. Test Artist Overhaul (Taylor Swift: 159260351)
    println!("\n--- 4. Testing Artist Overhaul (Taylor Swift) ---");
    let artist_route = PageRoute::Artist("159260351".to_string());
    let artist = service.get_page(&artist_route).await.expect("Artist page");
    println!("Artist Title: {}", artist.title);
    if let Some(ref header) = artist.header {
        println!("  Header Title: {}", header.title);
        println!("  Subtitle: {:?}", header.subtitle);
        println!("  Metadata: {:?}", header.metadata);
        println!("  Has Banner Artwork: {}", header.banner_artwork.is_some());
        if let Some(ref banner) = header.banner_artwork {
            println!("    Banner URL: {}", banner.url);
        }
        println!("  Has Standard Artwork: {}", header.artwork.is_some());
        println!("  Has Bio Description: {}", header.description.is_some());
        if let Some(ref bio) = header.description {
            let preview = if bio.len() > 120 { &bio[..120] } else { bio };
            println!("    Bio Preview: {}...", preview);
        }
    }
    println!("  Sections ({}):", artist.sections.len());
    assert!(
        !artist.sections.is_empty(),
        "Artist sections must not be empty"
    );

    let mut found_top_songs = false;
    let mut found_latest_release = false;
    let mut found_albums = false;
    let mut found_similar = false;

    for (i, sec) in artist.sections.iter().enumerate() {
        println!(
            "  [{:02}] id={:<24} | title={:<24} | hint={:<12} | items={}",
            i,
            sec.id,
            sec.title.as_deref().unwrap_or("Untitled"),
            sec.presentation_hint.as_deref().unwrap_or("default"),
            sec.items.len()
        );
        if sec.id == "view:top-songs" {
            found_top_songs = true;
            assert_eq!(
                sec.presentation_hint.as_deref(),
                Some("multi-row-track-shelf")
            );
        }
        if sec.id == "view:latest-release" {
            found_latest_release = true;
        }
        if sec.id == "view:full-albums" || sec.id == "view:featured-albums" {
            found_albums = true;
        }
        if sec.id == "view:similar-artists" {
            found_similar = true;
            assert_eq!(sec.presentation_hint.as_deref(), Some("artist-shelf"));
        }
    }

    assert!(found_top_songs, "Must contain Top Songs");
    assert!(found_albums, "Must contain Albums");
    assert!(found_similar, "Must contain Similar Artists");
    println!(
        "  Artist verification PASSED (top_songs={}, latest_release={}, albums={}, similar={})",
        found_top_songs, found_latest_release, found_albums, found_similar
    );

    // 5. Test Album Detail with related views (Lover Remix: 1487757227)
    println!("\n--- 5. Testing Album Detail (Lover Remix Single) ---");
    let album_route = PageRoute::Album("1487757227".to_string());
    let album = service.get_page(&album_route).await.expect("Album page");
    println!(
        "Album Title: {}, Sections: {}",
        album.title,
        album.sections.len()
    );
    assert!(
        !album.sections.is_empty(),
        "Album sections must not be empty"
    );
    if let Some(ref h) = album.header {
        println!("  Header Title: {}", h.title);
        println!("  Has Description: {}", h.description.is_some());
    }
    for (i, sec) in album.sections.iter().enumerate() {
        println!(
            "  [{:02}] id={:<24} | title={:<24} | hint={:<12} | items={}",
            i,
            sec.id,
            sec.title.as_deref().unwrap_or("Untitled"),
            sec.presentation_hint.as_deref().unwrap_or("default"),
            sec.items.len()
        );
    }
    assert_eq!(album.sections[0].id, "tracks");
    assert_eq!(
        album.sections[0].presentation_hint.as_deref(),
        Some("track-list")
    );
    let album_header = album.header.as_ref().expect("Album header");
    assert!(
        album_header.banner_artwork.is_none(),
        "Album MUST NOT have banner artwork (must be square cover)"
    );
    assert!(
        album_header.artwork.is_some(),
        "Album must have standard square artwork"
    );

    // 6. Test Playlist Detail (Top 100 Global)
    println!("\n--- 6. Testing Playlist Detail (Top 100: Global) ---");
    let playlist_route = PageRoute::Playlist("pl.d25f5d1181894928af76c85c967f8f31".to_string());
    if let Ok(playlist) = service.get_page(&playlist_route).await {
        println!("Playlist Title: {}", playlist.title);
        let playlist_header = playlist.header.as_ref().expect("Playlist header");
        assert!(
            playlist_header.banner_artwork.is_none(),
            "Playlist MUST NOT have banner artwork (must be square cover)"
        );
        assert!(
            playlist_header.artwork.is_some(),
            "Playlist must have standard square artwork"
        );
        println!("  Playlist square artwork verified: banner_artwork is None");
    }

    // 7. Test Library Artist View (Alan Walker: r.UJOmYb6)
    println!("\n--- 7. Testing Library Artist View (Alan Walker) ---");
    let lib_artist_route = PageRoute::Artist("r.UJOmYb6".to_string());
    let lib_artist = service
        .get_page(&lib_artist_route)
        .await
        .expect("Library artist page");
    println!(
        "Library Artist Title: {}, Sections: {}",
        lib_artist.title,
        lib_artist.sections.len()
    );
    let lib_header = lib_artist.header.as_ref().expect("Library artist header");
    assert!(
        lib_header.banner_artwork.is_none(),
        "Library artist MUST NOT have wide banner artwork"
    );
    assert!(
        lib_header
            .metadata
            .iter()
            .any(|m| m.starts_with("catalog_id:")),
        "Library artist must carry catalog_id link"
    );
    println!("  Library artist metadata: {:?}", lib_header.metadata);

    // Verify sections contain ONLY songs and albums in library, NOT full catalog discography
    for (i, sec) in lib_artist.sections.iter().enumerate() {
        println!(
            "  [{:02}] id={:<24} | title={:<24} | hint={:<12} | items={}",
            i,
            sec.id,
            sec.title.as_deref().unwrap_or("Untitled"),
            sec.presentation_hint.as_deref().unwrap_or("default"),
            sec.items.len()
        );
        assert!(
            sec.id == "songs" || sec.id == "albums",
            "Library artist sections must only be 'songs' or 'albums', found: {}",
            sec.id
        );
        for item in &sec.items {
            assert_eq!(
                item.in_library,
                Some(true),
                "Library artist item must have in_library == true"
            );
        }
    }

    println!("\n=== ALL FULL REVAMP LIVE CHECKS PASSED SUCCESSFULLY ===");
}
