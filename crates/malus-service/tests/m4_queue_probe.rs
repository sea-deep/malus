//! M4 MusicKit Queue Behavior Probe
//!
//! Live test that explores MusicKit's actual queue API surface at runtime.
//! Uses the authenticated WPE session to determine real setQueue syntax,
//! queue state extraction, and queue properties.
//!
//! Run with:
//!   cargo test -p malus-service --test m4_queue_probe -- --ignored --nocapture

use std::{sync::Arc, time::Duration};

use malus_model::{MediaRef, PlaybackState, Queue};
use malus_service::{AppleService, AppleWebSession, ProductionAppleWebSession};
use malus_wpe::ProfileManager;

fn print_queue(label: &str, queue: &Queue) {
    println!(
        "  {label}: {} items, current_index={:?}",
        queue.items.len(),
        queue.current_index
    );
    for (i, track) in queue.items.iter().enumerate() {
        let marker = if queue.current_index == Some(i) {
            ">"
        } else {
            " "
        };
        println!(
            "    {} {:2}. [{}] {} - {}",
            marker,
            i + 1,
            track.id,
            track.title,
            track.artist_display()
        );
    }
}

#[tokio::test]
#[ignore = "live MusicKit queue probe requiring authenticated profile"]
async fn probe_musickit_queue_behavior() {
    println!("\n=== M4 MusicKit Queue Behavior Probe ===\n");

    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    assert!(
        token_path.exists(),
        "Authentication tokens.json must exist in apple profile"
    );

    let session: Arc<dyn AppleWebSession> = Arc::new(ProductionAppleWebSession::new());
    let service = AppleService::with_session(session.clone());

    // ================================================================
    // PROBE 1: setQueue single song
    // ================================================================
    println!("--- PROBE 1: setQueue single song ---");
    let song_ref = MediaRef::Song("617154362".to_string());
    service.play(&song_ref).await.expect("play song");

    let mut playing = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let status = service.get_status().await.expect("status");
        if status.state == PlaybackState::Playing {
            playing = true;
            println!(
                "  Song playing: {:?}",
                status.current_track.as_ref().map(|t| &t.title)
            );
            break;
        }
    }
    assert!(playing, "Song did not start playing");

    // Extract queue after song play
    let queue = service.get_queue().await.expect("get_queue after song");
    print_queue("Song queue", &queue);
    assert!(
        !queue.items.is_empty(),
        "Queue should have items after song play"
    );

    // Verify pause/resume/stop still work
    println!("\n  Testing pause...");
    service.pause().await.expect("pause");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = service.get_status().await.expect("status after pause");
    assert_eq!(status.state, PlaybackState::Paused, "should be paused");

    println!("  Testing resume...");
    service.resume().await.expect("resume");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = service.get_status().await.expect("status after resume");
    assert_eq!(status.state, PlaybackState::Playing, "should be playing");

    // Stop
    service.stop().await.expect("stop");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ================================================================
    // PROBE 2: setQueue album
    // ================================================================
    println!("\n--- PROBE 2: setQueue album ---");
    // Discovery (1997) by Daft Punk
    let album_ref = MediaRef::Album("1440833098".to_string());
    service.play(&album_ref).await.expect("play album");

    let mut playing = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let status = service.get_status().await.expect("status");
        if status.state == PlaybackState::Playing {
            playing = true;
            println!(
                "  Album playing: {:?}",
                status.current_track.as_ref().map(|t| &t.title)
            );
            break;
        }
    }
    assert!(playing, "Album did not start playing");

    // Extract queue after album play
    let album_queue = service.get_queue().await.expect("get_queue after album");
    print_queue("Album queue", &album_queue);
    assert!(
        album_queue.items.len() > 1,
        "Album queue should have multiple tracks, got {}",
        album_queue.items.len()
    );
    assert!(
        album_queue.current_index.is_some(),
        "current_index should be set"
    );

    // Stop
    service.stop().await.expect("stop");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ================================================================
    // SUMMARY
    // ================================================================
    println!("\n=== Queue Probe Summary ===");
    println!("  Song setQueue: WORKS");
    println!("  Song queue items: {}", queue.items.len());
    println!("  Album setQueue: WORKS");
    println!("  Album queue items: {}", album_queue.items.len());
    println!("  Album current_index: {:?}", album_queue.current_index);
    println!("  Pause/Resume/Stop: WORKS");

    let _ = session.shutdown().await;
    println!("\n=== Probe Complete ===\n");
}
