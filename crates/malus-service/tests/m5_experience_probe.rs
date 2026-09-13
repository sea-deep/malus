//! M5 Apple Experience Features Probe
//!
//! Live test that verifies the M5 milestone against Apple's live APIs:
//! - Time-synced & syllable-synced lyrics
//! - Song credits and categories
//! - Favorites & unfavorite mutations
//! - Suggest-less & clear rating mutations
//! - Library addition & state reflection
//! - Playback resilience during metadata/mutation requests
//!
//! Run with:
//!   cargo test -p malus-service --test m5_experience_probe -- --ignored --nocapture

use std::{sync::Arc, time::Duration};

use malus_model::{MediaRef, PlaybackState, Rating};
use malus_service::{AppleService, AppleWebSession, ProductionAppleWebSession};
use malus_wpe::ProfileManager;

#[tokio::test]
#[ignore = "live Apple experience features probe requiring authenticated profile"]
async fn probe_apple_experience_features() {
    println!("\n=== M5 Apple Experience Features Probe ===\n");

    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    assert!(
        token_path.exists(),
        "Authentication tokens.json must exist in apple profile"
    );

    let session: Arc<dyn AppleWebSession> = Arc::new(ProductionAppleWebSession::new());
    let service = AppleService::with_session(session.clone());

    let test_song_ref = MediaRef::Song("6804643315".to_string()); // "Aasa Kooda"
    let fallback_song_ref = MediaRef::Song("1440857781".to_string()); // "Get Lucky"

    // ================================================================
    // PROBE 1: Lyrics (time-synced & syllable-synced)
    // ================================================================
    println!("--- PROBE 1: Fetch Lyrics ---");
    let lyrics = match service.get_lyrics(&test_song_ref).await {
        Ok(l) => l,
        Err(e) => {
            println!("  Fallback to Get Lucky due to: {e}");
            service
                .get_lyrics(&fallback_song_ref)
                .await
                .expect("lyrics")
        }
    };

    println!(
        "  Parsed {} lines, time-synced={}",
        lyrics.lines.len(),
        lyrics.synced
    );
    assert!(!lyrics.lines.is_empty(), "Lyrics must have lines");
    for (i, line) in lyrics.lines.iter().take(5).enumerate() {
        println!(
            "    Line {}: [{}ms - {}ms] {} (syllables: {})",
            i + 1,
            line.start_ms.unwrap_or(0),
            line.end_ms.unwrap_or(0),
            line.text,
            line.syllables.as_ref().map(|s| s.len()).unwrap_or(0)
        );
    }

    // ================================================================
    // PROBE 2: Credits
    // ================================================================
    println!("\n--- PROBE 2: Fetch Song Credits ---");
    let credits = service
        .get_credits(&test_song_ref)
        .await
        .expect("song credits");
    println!("  Parsed {} categories", credits.categories.len());
    assert!(
        !credits.categories.is_empty(),
        "Credits must have categories"
    );
    for cat in &credits.categories {
        println!("  Category: {}", cat.title);
        for item in &cat.items {
            println!("    - {}: {}", item.name, item.roles.join(", "));
        }
    }

    // ================================================================
    // PROBE 3: Account Media State (reflection)
    // ================================================================
    println!("\n--- PROBE 3: Query Account Media State ---");
    let state = service
        .get_media_state(&test_song_ref)
        .await
        .expect("initial media state");
    println!(
        "  Initial State: in_library={}, rating={:?}",
        state.in_library, state.rating
    );

    // ================================================================
    // PROBE 4: Favorite & Unfavorite
    // ================================================================
    println!("\n--- PROBE 4: Favorite & Unfavorite Mutation Cycle ---");
    println!("  Executing Favorite...");
    let state_fav = service.favorite(&test_song_ref).await.expect("favorite");
    println!(
        "  Post-Favorite State: in_library={}, rating={:?}, is_favorite={}",
        state_fav.in_library,
        state_fav.rating,
        state_fav.is_favorite()
    );
    assert!(state_fav.is_favorite(), "State must reflect favorite");

    println!("  Executing Unfavorite...");
    let state_unfav = service
        .unfavorite(&test_song_ref)
        .await
        .expect("unfavorite");
    println!(
        "  Post-Unfavorite State: in_library={}, rating={:?}, is_favorite={}",
        state_unfav.in_library,
        state_unfav.rating,
        state_unfav.is_favorite()
    );
    assert!(!state_unfav.is_favorite(), "State must reflect unfavorite");

    // ================================================================
    // PROBE 5: Suggest Less & Clear Rating
    // ================================================================
    println!("\n--- PROBE 5: Suggest Less & Clear Rating Mutation Cycle ---");
    println!("  Executing Suggest Less...");
    let state_dislike = service
        .suggest_less(&test_song_ref)
        .await
        .expect("suggest less");
    println!(
        "  Post-Suggest-Less State: in_library={}, rating={:?}, is_suggest_less={}",
        state_dislike.in_library,
        state_dislike.rating,
        state_dislike.is_suggest_less()
    );
    assert!(
        state_dislike.is_suggest_less(),
        "State must reflect suggest less"
    );

    println!("  Executing Clear Rating...");
    let state_cleared = service
        .clear_rating(&test_song_ref)
        .await
        .expect("clear rating");
    println!(
        "  Post-Clear-Rating State: in_library={}, rating={:?}",
        state_cleared.in_library, state_cleared.rating
    );
    assert_eq!(
        state_cleared.rating,
        Rating::Neutral,
        "Rating must be cleared to Neutral"
    );

    // ================================================================
    // PROBE 6: Add to Library
    // ================================================================
    println!("\n--- PROBE 6: Add to Library ---");
    let state_added = service
        .add_to_library(&test_song_ref)
        .await
        .expect("add to library");
    println!(
        "  Post-Add-To-Library State: in_library={}, rating={:?}",
        state_added.in_library, state_added.rating
    );
    assert!(state_added.in_library, "Song must reflect added to library");

    // ================================================================
    // PROBE 7: Playback Resilience During Metadata / Mutation Requests
    // ================================================================
    println!("\n--- PROBE 7: Playback Resilience During Experience Requests ---");
    println!("  Starting song playback...");
    service.play(&test_song_ref).await.expect("play song");

    let mut playing = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let status = service.get_status().await.expect("status");
        if status.state == PlaybackState::Playing {
            playing = true;
            println!("  Playback started: position={}ms", status.position_ms);
            break;
        }
    }
    assert!(playing, "Playback should reach Playing state");

    println!("  Fetching lyrics during active playback...");
    let _ = service
        .get_lyrics(&test_song_ref)
        .await
        .expect("lyrics during play");

    println!("  Fetching credits during active playback...");
    let _ = service
        .get_credits(&test_song_ref)
        .await
        .expect("credits during play");

    println!("  Mutating favorites during active playback...");
    let _ = service
        .favorite(&test_song_ref)
        .await
        .expect("favorite during play");

    tokio::time::sleep(Duration::from_millis(1000)).await;
    let status = service.get_status().await.expect("status");
    println!(
        "  Verified playback state after experience calls: {:?} at {}ms",
        status.state, status.position_ms
    );
    assert_eq!(
        status.state,
        PlaybackState::Playing,
        "Playback must remain Playing without interruption"
    );

    println!("  Stopping playback...");
    service.stop().await.expect("stop");

    println!("\n=== ALL M5 APPLE EXPERIENCE PROBES PASSED! ===\n");
}
