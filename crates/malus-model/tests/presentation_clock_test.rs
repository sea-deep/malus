use malus_model::{PlaybackSample, PlaybackState, PresentationClock};
use std::time::{Duration, Instant};

#[test]
fn test_normal_continuous_playback() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    // Initial sample: playing at 10,000ms
    clock.update_at(
        PlaybackSample {
            position_ms: 10_000,
            duration_ms: 200_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 1,
        },
        t0,
    );

    // Monotonic advancement across ticks
    let mut last_pos = clock.position_ms_at(t0);
    assert_eq!(last_pos, 10_000);

    for ms in [50, 100, 150, 200, 250] {
        let t = t0 + Duration::from_millis(ms);
        let pos = clock.position_ms_at(t);
        assert!(
            pos >= last_pos,
            "Position must never decrease during continuous playback: was {last_pos}, now {pos}"
        );
        assert_eq!(pos, 10_000 + ms);
        last_pos = pos;
    }

    // A newer sample arrives with small audio buffer jitter (e.g. 40ms behind wall-clock extrapolation)
    let t_sample = t0 + Duration::from_millis(300);
    // At t0 + 300, clock would interpolate to 10,300ms.
    // Audio engine reports 10,260ms (40ms buffer latency).
    clock.update_at(
        PlaybackSample {
            position_ms: 10_260,
            duration_ms: 200_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 2,
        },
        t_sample,
    );

    // Position at t_sample must NOT regress to 10,260ms; it must remain at 10,300ms
    let pos_after_sample = clock.position_ms_at(t_sample);
    assert_eq!(pos_after_sample, 10_300);

    // Subsequent ticks advance smoothly forward from 10,300ms without accumulating artificial lead
    let pos_350 = clock.position_ms_at(t0 + Duration::from_millis(350));
    assert_eq!(pos_350, 10_310);
}

#[test]
fn test_out_of_order_samples() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 10_000,
            duration_ms: 100_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 1,
        },
        t0,
    );

    // Newer sample sequence 3 arrives
    let t1 = t0 + Duration::from_millis(200);
    clock.update_at(
        PlaybackSample {
            position_ms: 10_200,
            duration_ms: 100_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 3,
        },
        t1,
    );
    assert_eq!(clock.position_ms_at(t1), 10_200);

    // Stale/out-of-order sample sequence 2 arrives later at t1 + 50ms
    let t2 = t1 + Duration::from_millis(50);
    clock.update_at(
        PlaybackSample {
            position_ms: 10_100,
            duration_ms: 100_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 2, // Out-of-order! Older than current sequence 3
        },
        t2,
    );

    // Clock must ignore sequence 2 and continue interpolating from sequence 3
    let pos_at_t2 = clock.position_ms_at(t2);
    assert_eq!(pos_at_t2, 10_250);
}

#[test]
fn test_delayed_samples() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 5_000,
            duration_ms: 60_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 1,
        },
        t0,
    );

    // 1000ms passes without samples; clock interpolates to 6_000ms
    let t_delayed = t0 + Duration::from_millis(1000);
    assert_eq!(clock.position_ms_at(t_delayed), 6_000);

    // Delayed sample arrives reporting 5_920ms (delayed in transit)
    clock.update_at(
        PlaybackSample {
            position_ms: 5_920,
            duration_ms: 60_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 2,
        },
        t_delayed,
    );

    // Clock must not regress to 5_920ms; must remain at 6_000ms
    assert_eq!(clock.position_ms_at(t_delayed), 6_000);

    // Subsequent tick continues forward without accumulating artificial lead
    assert_eq!(
        clock.position_ms_at(t_delayed + Duration::from_millis(100)),
        6_020
    );
}

#[test]
fn test_tiny_backward_seek() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 15_000,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 10,
        },
        t0,
    );

    // User seeks backward by just 50ms (from 15,000ms to 14,950ms)
    // Discontinuity increments timeline_id to 2
    let t_seek = t0 + Duration::from_millis(10);
    clock.update_at(
        PlaybackSample {
            position_ms: 14_950,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 2, // New timeline!
            sequence: 1,
        },
        t_seek,
    );

    // Must immediately reset to the authoritative seek target, even for a tiny 50ms seek
    assert_eq!(clock.position_ms_at(t_seek), 14_950);
}

#[test]
fn test_large_backward_seek() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 120_000, // 2:00
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 50,
        },
        t0,
    );

    // User seeks backward to 0:15 (15_000ms)
    let t_seek = t0 + Duration::from_millis(50);
    clock.update_at(
        PlaybackSample {
            position_ms: 15_000,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 2,
            sequence: 1,
        },
        t_seek,
    );

    assert_eq!(clock.position_ms_at(t_seek), 15_000);
}

#[test]
fn test_restart_to_zero() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 45_000,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 25,
        },
        t0,
    );

    // restart_current_item seeks to 0ms with a new timeline revision
    let t_restart = t0 + Duration::from_millis(20);
    clock.update_at(
        PlaybackSample {
            position_ms: 0,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 2,
            sequence: 1,
        },
        t_restart,
    );

    assert_eq!(clock.position_ms_at(t_restart), 0);
}

#[test]
fn test_repeat_one_wrap() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    // Track is at 179_900ms near duration 180_000ms
    clock.update_at(
        PlaybackSample {
            position_ms: 179_900,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 90,
        },
        t0,
    );

    // Repeat-One wrap: track ends and automatically loops back to 0ms
    let t_wrap = t0 + Duration::from_millis(100);
    clock.update_at(
        PlaybackSample {
            position_ms: 0,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 2, // Discontinuity on repeat wrap
            sequence: 1,
        },
        t_wrap,
    );

    assert_eq!(clock.position_ms_at(t_wrap), 0);
}

#[test]
fn test_next_previous() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 80_000,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 40,
        },
        t0,
    );

    // User skips to next song: timeline increments, new song starts at 0ms
    let t_next = t0 + Duration::from_millis(30);
    clock.update_at(
        PlaybackSample {
            position_ms: 0,
            duration_ms: 210_000,
            state: PlaybackState::Playing,
            timeline_id: 2,
            sequence: 1,
        },
        t_next,
    );

    assert_eq!(clock.position_ms_at(t_next), 0);
    assert_eq!(clock.duration_ms(), 210_000);

    // Plays for 5_000ms with normal intermediate status heartbeat
    let t_mid = t_next + Duration::from_millis(2_500);
    clock.update_at(
        PlaybackSample {
            position_ms: 2_500,
            duration_ms: 210_000,
            state: PlaybackState::Playing,
            timeline_id: 2,
            sequence: 2,
        },
        t_mid,
    );
    let t_prev = t_next + Duration::from_millis(5_000);
    assert_eq!(clock.position_ms_at(t_prev), 5_000);

    // User skips back to previous song
    clock.update_at(
        PlaybackSample {
            position_ms: 0,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 3,
            sequence: 1,
        },
        t_prev,
    );

    assert_eq!(clock.position_ms_at(t_prev), 0);
    assert_eq!(clock.duration_ms(), 180_000);
}

#[test]
fn test_pause_resume() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    // Playing at 5_000ms
    clock.update_at(
        PlaybackSample {
            position_ms: 5_000,
            duration_ms: 100_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 1,
        },
        t0,
    );

    // Advances for 500ms
    let t_pause = t0 + Duration::from_millis(500);
    assert_eq!(clock.position_ms_at(t_pause), 5_500);

    // User pauses
    clock.update_at(
        PlaybackSample {
            position_ms: 5_500,
            duration_ms: 100_000,
            state: PlaybackState::Paused,
            timeline_id: 1,
            sequence: 2,
        },
        t_pause,
    );

    // While paused, position must freeze and never advance
    for ms in [100, 500, 1000, 2000] {
        let t = t_pause + Duration::from_millis(ms);
        assert_eq!(
            clock.position_ms_at(t),
            5_500,
            "Position must freeze while paused"
        );
    }

    // User resumes playback after 2000ms
    let t_resume = t_pause + Duration::from_millis(2000);
    clock.update_at(
        PlaybackSample {
            position_ms: 5_500,
            duration_ms: 100_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 3,
        },
        t_resume,
    );

    // Resumes linear advancement from 5_500ms
    let t_after = t_resume + Duration::from_millis(300);
    assert_eq!(clock.position_ms_at(t_after), 5_800);
}

#[test]
fn test_no_runaway_drift_on_noisy_samples() {
    let mut clock = PresentationClock::new();
    let t0 = Instant::now();

    clock.update_at(
        PlaybackSample {
            position_ms: 0,
            duration_ms: 180_000,
            state: PlaybackState::Playing,
            timeline_id: 1,
            sequence: 1,
        },
        t0,
    );

    // Simulate 50 sample heartbeats over 25 seconds, each with variable audio buffer latency (20-50ms behind wall-clock)
    let mut current_t = t0;
    for seq in 2..=50 {
        current_t += Duration::from_millis(500);
        let wall_elapsed_ms = current_t.duration_since(t0).as_millis() as u64;
        // Audio engine reports audio time with 30ms latency
        let audio_pos_ms = wall_elapsed_ms.saturating_sub(30);

        clock.update_at(
            PlaybackSample {
                position_ms: audio_pos_ms,
                duration_ms: 180_000,
                state: PlaybackState::Playing,
                timeline_id: 1,
                sequence: seq,
            },
            current_t,
        );

        let presented = clock.position_ms_at(current_t);
        // Error between presented time and true audio position must stay strictly bounded within jitter (<= 50ms)
        // and NEVER accumulate runaway lead (which previously caused lyrics to activate seconds ahead).
        assert!(
            presented >= audio_pos_ms,
            "Presentation must not regress below audio position"
        );
        let drift = presented - audio_pos_ms;
        assert!(
            drift <= 50,
            "Drift must remain tightly bounded (got {drift}ms at seq {seq})"
        );
    }
}
