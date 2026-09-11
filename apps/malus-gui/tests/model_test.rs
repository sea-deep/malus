use malus_client::{PlaybackStateWire, ProviderInfoWire, TrackWire};
use malus_gui::model::{
    NavigationHistory, PlayerModel, ProviderCache, Route, SearchSession, format_time,
};
use std::time::Duration;

#[test]
fn test_navigation_history() {
    let mut nav = NavigationHistory::default();
    assert_eq!(*nav.current(), Route::Search);
    assert!(!nav.can_go_back());
    assert!(!nav.can_go_forward());

    nav.navigate_to(Route::AlbumDetail("mock:album:1".into()));
    assert_eq!(*nav.current(), Route::AlbumDetail("mock:album:1".into()));
    assert!(nav.can_go_back());
    assert!(!nav.can_go_forward());

    nav.navigate_to(Route::ArtistDetail("mock:artist:1".into()));
    assert_eq!(*nav.current(), Route::ArtistDetail("mock:artist:1".into()));
    assert!(nav.can_go_back());

    // Go back
    assert_eq!(
        nav.go_back(),
        Some(&Route::AlbumDetail("mock:album:1".into()))
    );
    assert_eq!(*nav.current(), Route::AlbumDetail("mock:album:1".into()));
    assert!(nav.can_go_forward());

    assert_eq!(nav.go_back(), Some(&Route::Search));
    assert_eq!(*nav.current(), Route::Search);
    assert!(!nav.can_go_back());
    assert!(nav.can_go_forward());

    // Go forward
    assert_eq!(
        nav.go_forward(),
        Some(&Route::AlbumDetail("mock:album:1".into()))
    );
    assert_eq!(*nav.current(), Route::AlbumDetail("mock:album:1".into()));

    // Navigating to a new route clears forward history
    nav.navigate_to(Route::Search);
    assert!(!nav.can_go_forward());
}

#[test]
fn test_search_session_generation() {
    let mut session = SearchSession::new("apple");
    assert_eq!(session.generation, 0);

    let g1 = session.set_query("daft punk".into());
    assert_eq!(g1, 1);
    assert_eq!(session.query, "daft punk");

    let g2 = session.set_provider("mock".into());
    assert_eq!(g2, 2);
    assert_eq!(session.provider, "mock");
}

#[test]
fn test_player_monotonic_extrapolation() {
    let mut player = PlayerModel::default();
    assert_eq!(player.extrapolated_position_ms(), 0);

    let track = TrackWire::new("apple:track:123", "Get Lucky", "Daft Punk");
    player.update_from_wire(
        PlaybackStateWire::Playing,
        Some(track),
        10_000,
        Some(240_000),
    );

    assert_eq!(player.playback_provider.as_deref(), Some("apple"));
    std::thread::sleep(Duration::from_millis(50));
    let extrapolated = player.extrapolated_position_ms();
    assert!(
        extrapolated >= 10_040,
        "Extrapolated position {extrapolated} should be >= 10040"
    );

    // When paused, position does not advance
    player.update_from_wire(
        PlaybackStateWire::Paused,
        player.current_track.clone(),
        15_000,
        Some(240_000),
    );
    let paused_pos = player.extrapolated_position_ms();
    assert_eq!(paused_pos, 15_000);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(player.extrapolated_position_ms(), 15_000);
}

#[test]
fn test_provider_cache_and_capabilities() {
    let mut cache = ProviderCache::default();
    assert!(!cache.has_capability("apple", "playback"));

    let apple_info = ProviderInfoWire {
        id: "apple".into(),
        name: "Apple Music".into(),
        state: "ready".into(),
        capabilities: vec![
            "catalog".into(),
            "search".into(),
            "auth".into(),
            "library.tracks".into(),
            "library.albums".into(),
            "library.playlists".into(),
            "playback".into(),
            "playback.seek".into(),
        ],
        auth_state: None,
    };
    let mock_info = ProviderInfoWire {
        id: "mock".into(),
        name: "Mock Provider".into(),
        state: "ready".into(),
        capabilities: vec![
            "catalog".into(),
            "search".into(),
            "playback".into(),
            "playback.seek".into(),
        ],
        auth_state: None,
    };

    cache.set_providers(vec![apple_info, mock_info]);

    assert!(cache.has_capability("apple", "auth"));
    assert!(!cache.has_capability("mock", "auth"));
    assert!(cache.has_capability("apple", "library.tracks"));
    assert!(!cache.has_capability("mock", "library.tracks"));
    assert!(cache.has_capability("apple", "playback.seek"));
    assert!(cache.has_capability("mock", "playback.seek"));
    assert!(!cache.has_capability("apple", "playback.volume"));
}

#[test]
fn test_format_time() {
    assert_eq!(format_time(0), "0:00");
    assert_eq!(format_time(5_000), "0:05");
    assert_eq!(format_time(65_000), "1:05");
    assert_eq!(format_time(600_000), "10:00");
}
