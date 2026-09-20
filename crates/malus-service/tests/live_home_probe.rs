//! Live acceptance probe for Apple Music Home page.
//! Run with: cargo test -p malus-service --test live_home_probe -- --ignored --nocapture

use malus_model::{MediaRef, PageRoute};
use malus_service::{
    AppleService, OfficialAppleMusicApi, ProductionAppleWebSession, ProfileTokenProvider,
};
use malus_wpe::ProfileManager;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires real authenticated Apple Music profile"]
async fn test_live_home_page_acceptance() {
    println!("\n=== LIVE APPLE MUSIC HOME ACCEPTANCE PROBE ===\n");

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

    let page = service
        .get_page(&PageRoute::Home)
        .await
        .expect("Live Home page fetched");
    println!("Page ID: {}", page.id);
    println!("Page Title: {}", page.title);
    println!("Total Shelves Returned: {}\n", page.sections.len());

    assert_eq!(page.id, "home");
    assert_eq!(page.title, "Home");
    assert!(!page.sections.is_empty(), "Home sections must not be empty");

    let mut found_top_picks = false;
    let mut found_recently_played = false;
    let mut found_stations = false;
    let mut found_replay = false;

    for (idx, section) in page.sections.iter().enumerate() {
        let title = section.title.as_deref().unwrap_or("Untitled");
        let hint = section.presentation_hint.as_deref().unwrap_or("shelf");
        let item_count = section.items.len();
        let subtitle = section.subtitle.as_deref().unwrap_or("");

        let sample_types: Vec<String> = section
            .items
            .iter()
            .take(3)
            .map(|it| match &it.entity {
                Some(MediaRef::Song(_)) => "song".to_string(),
                Some(MediaRef::Album(_)) => "album".to_string(),
                Some(MediaRef::Artist(_)) => "artist".to_string(),
                Some(MediaRef::Playlist(_)) => "playlist".to_string(),
                Some(MediaRef::Station(_)) => "station".to_string(),
                None => "unknown".to_string(),
            })
            .collect();

        println!(
            "[{:02}] hint={:<16} | items={:<2} | title={:<32} | samples={:?}",
            idx, hint, item_count, title, sample_types
        );
        if !subtitle.is_empty() {
            println!("     reason: {}", subtitle);
        }

        if hint == "top-picks-shelf" {
            found_top_picks = true;
            // Check that items have editorial notes or overlines
            let has_editorial = section.items.iter().any(|it| it.tertiary_text.is_some());
            println!("     has_editorial_notes: {}", has_editorial);
        }
        if title.to_lowercase().contains("recently played") {
            found_recently_played = true;
        }
        if hint == "stations-shelf" {
            found_stations = true;
        }
        if title.to_lowercase().contains("replay") {
            found_replay = true;
        }
    }

    assert!(found_top_picks, "Must contain Top Picks hero shelf");
    assert!(
        found_recently_played,
        "Must contain Recently Played shelf natively"
    );
    assert!(
        found_stations,
        "Must contain stations shelf with station semantics"
    );
    assert!(
        !found_replay,
        "Replay shelf must be excluded for this milestone"
    );

    println!("\n=== Live Home acceptance probe completed successfully! ===\n");
}
