use malus::{
    app::{AppState, Msg, Overlay, Screen},
    controller::App,
    engine::{EngineCommand, MusicKitEvent},
    model::{AudioFormat, PlaybackStatus, Track},
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use ratcn::{
    Theme,
    runtime::{
        Event, KeyCode, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseKind, ScrollDirection,
    },
};
use std::time::Duration;
fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| {
            app.draw(
                f,
                &ui::theme(&Theme::default_dark()),
                Duration::from_secs(2),
            )
        })
        .unwrap();
    terminal
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::from(code), Duration::from_secs(2));
}
fn mouse(app: &mut App, x: u16, y: u16, kind: MouseKind) {
    app.handle_event(
        Event::Mouse(MouseEvent {
            column: x,
            row: y,
            kind,
            modifiers: Modifiers::NONE,
        }),
        Duration::from_secs(2),
    );
}
fn click(app: &mut App, x: u16, y: u16) {
    mouse(app, x, y, MouseKind::Down(MouseButton::Left));
    mouse(app, x, y, MouseKind::Up(MouseButton::Left));
}
fn text(t: &Terminal<TestBackend>) -> String {
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn real_startup_contains_no_sample_music() {
    let s = AppState::new();
    assert!(s.library.tracks.is_empty());
    assert!(s.library.albums.is_empty());
    assert!(s.player.current_track.is_none());
    assert!(s.player.current_lyrics().is_empty());
}
#[test]
fn queue_shortcut_and_space_work_even_with_a_focused_button() {
    let mut app = App::new(AppState::demo());
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Char('q'));
    assert_eq!(app.state.active_overlay, Some(Overlay::Queue));
    assert!(!app.state.should_quit);
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('4'));
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.state.player.status, PlaybackStatus::Playing);
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Tab);
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Char(' '));
    assert_eq!(app.state.player.status, PlaybackStatus::Paused);
}
#[test]
fn list_wheel_scrolls_and_does_not_change_volume() {
    let mut app = App::new(AppState::demo());
    app.state.navigate_to(Screen::Library);
    draw(&mut app, 100, 24);
    let vol = app.state.player.volume;
    mouse(&mut app, 20, 12, MouseKind::Scroll(ScrollDirection::Down));
    assert!(app.state.cursor.selected > 0);
    assert_eq!(vol, app.state.player.volume);
}
#[test]
fn search_text_does_not_trigger_hotkeys_and_enter_uses_selection() {
    let mut app = App::new(AppState::demo());
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Char('/'));
    draw(&mut app, 120, 34);
    for c in "the".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    draw(&mut app, 120, 34);
    key(&mut app, KeyCode::Down);
    draw(&mut app, 120, 34);
    let selected = app.state.filtered_search_results()[1].id.clone();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.state.player.current_track.unwrap().id, selected);
    assert!(app.state.active_overlay.is_none());
}
#[test]
fn modal_prevents_click_through_and_restores_focus() {
    let mut app = App::new(AppState::demo());
    app.state.navigate_to(Screen::Library);
    draw(&mut app, 120, 34);
    let before = app.state.focus.clone();
    key(&mut app, KeyCode::Char('?'));
    draw(&mut app, 120, 34);
    click(&mut app, 20, 1);
    assert!(app.state.active_overlay.is_none());
    assert_eq!(app.state.active_screen, Screen::Library);
    assert_eq!(app.state.focus, before);
}
#[test]
fn dragging_slider_seeks_without_activating_other_controls() {
    let mut app = App::new(AppState::demo());
    app.state
        .update(Msg::PlayTrack("trk-1".into()), Duration::ZERO);
    draw(&mut app, 100, 24);
    mouse(&mut app, 10, 22, MouseKind::Down(MouseButton::Left));
    draw(&mut app, 100, 24);
    mouse(&mut app, 60, 22, MouseKind::Drag(MouseButton::Left));
    mouse(&mut app, 60, 22, MouseKind::Up(MouseButton::Left));
    assert!(app.state.player.elapsed_secs > 100);
    assert!(!app.state.should_quit);
}
#[test]
fn long_lists_reach_last_song() {
    let mut s = AppState::demo();
    s.library.replace_tracks(
        (0..250)
            .map(|i| {
                Track::new(
                    i.to_string(),
                    format!("Song {i}"),
                    "Artist",
                    "Album",
                    120,
                    1,
                    AudioFormat::Standard,
                )
            })
            .collect(),
    );
    s.navigate_to(Screen::Library);
    let mut app = App::new(s);
    draw(&mut app, 80, 24);
    key(&mut app, KeyCode::End);
    let terminal = draw(&mut app, 80, 24);
    assert!(text(&terminal).contains("Song 249"));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.state.player.current_track.unwrap().id, "249");
}
#[test]
fn all_screens_and_modals_fit_small_and_large_terminals() {
    for (w, h) in [(20, 6), (40, 12), (60, 18), (80, 24), (120, 34), (180, 50)] {
        for screen in [
            Screen::ListenNow,
            Screen::Library,
            Screen::Browse,
            Screen::Radio,
            Screen::NowPlaying,
        ] {
            let mut app = App::new(AppState::demo());
            app.state.navigate_to(screen);
            draw(&mut app, w, h);
            for overlay in [
                Overlay::Search,
                Overlay::Queue,
                Overlay::CommandPalette,
                Overlay::Help,
                Overlay::Settings,
                Overlay::Lyrics,
                Overlay::ContextMenu("trk-1".into()),
            ] {
                app.state.update(Msg::OpenOverlay(overlay), Duration::ZERO);
                draw(&mut app, w, h);
                app.state.update(Msg::CloseOverlay, Duration::ZERO);
            }
        }
    }
}
#[test]
fn search_is_debounced_stale_results_are_ignored_and_library_is_unchanged() {
    let mut s = AppState::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    s.engine_tx = Some(tx);
    for (c, i) in "hello".chars().zip(0..) {
        s.update(Msg::SearchInput(c), Duration::from_millis(i * 20));
    }
    assert!(rx.try_recv().is_err());
    s.update(Msg::Tick, Duration::from_millis(500));
    assert!(matches!(rx.try_recv(),Ok(EngineCommand::Search(q)) if q=="hello"));
    let track = Track::new("1", "Hello", "A", "B", 10, 1, AudioFormat::Standard);
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::SearchResults {
            query: "hel".into(),
            tracks: vec![track.clone()],
        }),
        Duration::from_millis(600),
    );
    assert!(s.search_results.is_empty());
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::SearchResults {
            query: "hello".into(),
            tracks: vec![track],
        }),
        Duration::from_millis(650),
    );
    assert_eq!(s.search_results.len(), 1);
    assert!(s.library.tracks.is_empty());
}
#[test]
fn live_playback_waits_for_engine_confirmation_and_uses_catalog_id() {
    let mut s = AppState::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    s.engine_tx = Some(tx);
    let mut t = Track::new("i.library", "Song", "A", "B", 120, 1, AudioFormat::Standard);
    t.catalog_id = Some("123".into());
    s.library.replace_tracks(vec![t]);
    s.update(Msg::PlayTrack("i.library".into()), Duration::ZERO);
    assert!(matches!(rx.try_recv(),Ok(EngineCommand::PlayTracks(ids)) if ids==vec!["123"]));
    assert_eq!(s.player.status, PlaybackStatus::Stopped);
}
#[test]
fn albums_are_rebuilt_from_live_tracks_and_empty_library_clears_previous_data() {
    let mut s = AppState::demo();
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::LibraryLoaded { tracks: vec![] }),
        Duration::ZERO,
    );
    assert!(s.library.tracks.is_empty());
    assert!(s.library.albums.is_empty());
    assert!(s.library.artists.is_empty());
}
#[test]
fn ctrl_c_always_quits_inside_search() {
    let mut app = App::new(AppState::demo());
    app.state
        .update(Msg::OpenOverlay(Overlay::Search), Duration::ZERO);
    draw(&mut app, 80, 24);
    app.handle_event(
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        }),
        Duration::ZERO,
    );
    assert!(app.state.should_quit);
}

#[test]
fn library_relationships_group_collaborations_into_the_correct_album() {
    use malus::engine::musickit::parse_musickit_track;
    use serde_json::json;
    let song = |id: &str, artist: &str, number: u32| {
        parse_musickit_track(&json!({"id":id,"attributes":{"name":id,"artistName":artist,"albumName":"Shared album","trackNumber":number,"durationInMillis":120000,"playParams":{"catalogId":"catalog-id"}},"relationships":{"albums":{"data":[{"id":"l.album","attributes":{"artistName":"Main artist"}}]},"artists":{"data":[{"id":"r.artist","attributes":{"name":"Main artist"}}]}}})).unwrap()
    };
    let mut s = AppState::new();
    s.library.replace_tracks(vec![
        song("second", "Main artist & Guest B", 2),
        song("first", "Main artist & Guest A", 1),
    ]);
    assert_eq!(s.library.albums.len(), 1);
    assert_eq!(s.library.albums[0].id, "l.album");
    assert_eq!(s.library.albums[0].artist, "Main artist");
    assert_eq!(s.library.albums[0].track_ids, vec!["first", "second"]);
    assert_eq!(s.library.artists.len(), 1);
}
#[test]
fn playlist_loading_does_not_pollute_charts_or_the_library() {
    let mut s = AppState::new();
    let track = Track::new(
        "playlist-song",
        "Song",
        "Artist",
        "Album",
        100,
        1,
        AudioFormat::Standard,
    );
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::PlaylistLoaded {
            id: "p.1".into(),
            tracks: vec![track],
        }),
        Duration::ZERO,
    );
    assert!(s.catalog.is_empty());
    assert!(s.library.tracks.is_empty());
    assert!(s.find_track("playlist-song").is_some());
}

#[test]
fn failures_are_scoped_to_the_request_that_failed() {
    use malus::engine::musickit::DataRequest;
    let mut s = AppState::new();
    s.search_query = "latest".into();
    s.search_loading = true;
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::RequestFailed {
            request: DataRequest::Search("old".into()),
            message: "stale error".into(),
        }),
        Duration::ZERO,
    );
    assert!(s.search_loading);
    assert!(s.search_error.is_none());
    assert!(s.library_loading);
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::RequestFailed {
            request: DataRequest::Search("latest".into()),
            message: "Network unavailable".into(),
        }),
        Duration::ZERO,
    );
    assert!(!s.search_loading);
    assert!(s.search_error.is_some());
    assert!(s.library_loading);
    s.update(
        Msg::EngineMusicKitEvent(MusicKitEvent::Error {
            message: "Playback rejected".into(),
        }),
        Duration::ZERO,
    );
    assert!(s.library_loading);
}
#[test]
fn queue_can_be_reordered_with_the_keyboard() {
    let mut app = App::new(AppState::demo());
    app.state.player.queue = app.state.library.tracks.iter().take(3).cloned().collect();
    app.state
        .update(Msg::OpenOverlay(Overlay::Queue), Duration::ZERO);
    draw(&mut app, 120, 34);
    app.handle_event(
        Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
        }),
        Duration::ZERO,
    );
    assert_eq!(app.state.player.queue[0].id, "trk-2");
    assert_eq!(app.state.player.queue[1].id, "trk-1");
}
