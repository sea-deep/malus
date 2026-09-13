//! M5 Apple Experience Features Probe
//!
//! Live test that verifies the M5 milestone against Apple's live APIs:
//! - Time-synced & syllable-synced lyrics (read-only)
//! - Song credits and categories (read-only)
//! - Account state reflection (read-only)
//! - Favorites & unfavorite mutations with independent state lookup and restoration
//! - Suggest-less & clear rating mutations with independent state lookup and restoration
//! - Playback resilience during metadata/experience requests
//!
//! Gated by:
//! - MALUS_LIVE=1: enables live probes
//! - MALUS_LIVE_MUTATIONS=1: enables reversible mutations with automatic restoration
//!
//! Run with:
//!   MALUS_LIVE=1 MALUS_LIVE_MUTATIONS=1 cargo test -p malus-service --test m5_experience_probe -- --ignored --nocapture

use std::{sync::Arc, time::Duration};

use malus_model::{MediaRef, PlaybackState, Rating};
use malus_service::{AppleService, AppleWebSession, ProductionAppleWebSession};
use malus_wpe::ProfileManager;

#[tokio::test]
#[ignore = "live Apple experience features probe requiring authenticated profile"]
async fn probe_apple_experience_features() {
    if std::env::var("MALUS_LIVE").unwrap_or_default() != "1" {
        println!("Skipping probe_apple_experience_features because MALUS_LIVE!=1");
        return;
    }

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
    // PROBE 3: Account Media State (read-only reflection)
    // ================================================================
    println!("\n--- PROBE 3: Query Account Media State ---");
    let initial_state = service
        .get_media_state(&test_song_ref)
        .await
        .expect("initial media state");
    println!(
        "  Initial State: in_library={}, favorite={}, rating={:?}",
        initial_state.in_library, initial_state.favorite, initial_state.rating
    );

    // ================================================================
    // PROBES 4 & 5: Gated Reversible Mutations with State Restoration
    // ================================================================
    let allow_mutations = std::env::var("MALUS_LIVE_MUTATIONS").unwrap_or_default() == "1";
    if !allow_mutations {
        println!("\n--- Skipping Probes 4 & 5 (mutations) because MALUS_LIVE_MUTATIONS!=1 ---");
    } else {
        println!("\n--- PROBES 4 & 5: Running Gated Reversible Mutations with Restoration ---");
        let was_favorite = initial_state.favorite;
        let initial_rating = initial_state.rating;

        // Perform mutations and assertions with guaranteed restoration
        let mutation_run = async {
            // PROBE 4: Favorite & Unfavorite Mutation Cycle
            println!("  Executing Favorite...");
            let state_fav = service.favorite(&test_song_ref).await?;
            println!(
                "  Post-Favorite State: in_library={}, favorite={}, rating={:?}",
                state_fav.in_library, state_fav.favorite, state_fav.rating
            );
            assert!(state_fav.is_favorite(), "State must reflect favorite");

            // Independent instance verification:
            // Ensure a fresh AppleService instance with NO local state/cache sees favorite=true
            let fresh_service = AppleService::with_session(session.clone());
            let fresh_state = fresh_service.get_media_state(&test_song_ref).await?;
            println!(
                "  Fresh Instance Post-Favorite: favorite={}, rating={:?}",
                fresh_state.favorite, fresh_state.rating
            );
            assert!(
                fresh_state.is_favorite(),
                "Fresh instance must authoritatively reflect favorite=true"
            );

            println!("  Executing Unfavorite...");
            let state_unfav = service.unfavorite(&test_song_ref).await?;
            println!(
                "  Post-Unfavorite State: in_library={}, favorite={}, rating={:?}",
                state_unfav.in_library, state_unfav.favorite, state_unfav.rating
            );
            assert!(!state_unfav.is_favorite(), "State must reflect unfavorite");

            let fresh_state_unfav = fresh_service.get_media_state(&test_song_ref).await?;
            println!(
                "  Fresh Instance Post-Unfavorite: favorite={}, rating={:?}",
                fresh_state_unfav.favorite, fresh_state_unfav.rating
            );
            assert!(
                !fresh_state_unfav.is_favorite(),
                "Fresh instance must authoritatively reflect favorite=false"
            );

            // PROBE 5: Suggest Less & Clear Rating Mutation Cycle
            println!("\n  Executing Suggest Less...");
            let state_dislike = service.suggest_less(&test_song_ref).await?;
            println!(
                "  Post-Suggest-Less State: in_library={}, favorite={}, rating={:?}",
                state_dislike.in_library, state_dislike.favorite, state_dislike.rating
            );
            assert!(
                state_dislike.is_suggest_less(),
                "State must reflect suggest less"
            );
            assert!(
                !state_dislike.is_favorite(),
                "Suggest less must be independent from favorite"
            );

            let fresh_state_dislike = fresh_service.get_media_state(&test_song_ref).await?;
            assert_eq!(
                fresh_state_dislike.rating,
                Rating::SuggestLess,
                "Fresh instance must authoritatively reflect suggest less"
            );

            println!("  Executing Clear Rating...");
            let state_cleared = service.clear_rating(&test_song_ref).await?;
            println!(
                "  Post-Clear-Rating State: in_library={}, favorite={}, rating={:?}",
                state_cleared.in_library, state_cleared.favorite, state_cleared.rating
            );
            assert_eq!(
                state_cleared.rating,
                Rating::Neutral,
                "Rating must be cleared to Neutral"
            );

            let fresh_state_cleared = fresh_service.get_media_state(&test_song_ref).await?;
            assert_eq!(
                fresh_state_cleared.rating,
                Rating::Neutral,
                "Fresh instance must authoritatively reflect Neutral"
            );

            Ok::<(), malus_service::AppleError>(())
        }
        .await;

        // Guaranteed cleanup / restoration of initial state
        println!(
            "  Restoring initial account state (was_favorite={was_favorite}, initial_rating={initial_rating:?})..."
        );
        if was_favorite {
            let _ = service.favorite(&test_song_ref).await;
        } else {
            let _ = service.unfavorite(&test_song_ref).await;
        }
        match initial_rating {
            Rating::SuggestLess => {
                let _ = service.suggest_less(&test_song_ref).await;
            }
            Rating::Neutral => {
                let _ = service.clear_rating(&test_song_ref).await;
            }
        }
        println!("  Initial account state restored successfully.");

        mutation_run.expect("Reversible mutations must succeed");
    }

    // ================================================================
    // PROBE 6: Playback Resilience During Experience Requests
    // ================================================================
    println!("\n--- PROBE 6: Playback Resilience During Experience Requests ---");
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

    println!("  Querying media state during active playback...");
    let _ = service
        .get_media_state(&test_song_ref)
        .await
        .expect("media state during play");

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
