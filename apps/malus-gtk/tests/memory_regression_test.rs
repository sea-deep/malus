//! Memory audit and leak regression test.
//!
//! Verifies:
//! 1. Repeated navigation (Home -> Albums -> Detail -> Home) plateaus without leaking widgets/buffers.
//! 2. 20 track changes with artwork updates plateau without leaking textures/buffers.
//! 3. 10x Now Playing open/close cycles plateau without monotonic growth.

use malus_client::MalusClient;
use malus_gtk::{
    app::{AppInput, MalusApp},
    navigation::AppDestination,
    pages::now_playing::NowPlayingPage,
    services::DecodedImage,
    state::{NowPlayingMode, PlayerPresentation},
    widgets::player_controls::{CommandHandler, MenuHandler},
};
use malus_ipc::{
    client::{ClientRequest, ClientResponse},
    codec::{decode_message, read_frame, write_message},
    wire::{NavigationWire, PageItemWire, PageSectionWire, PageWire},
};
use malus_model::{Artwork, PageRoute, PlayerStatus, Queue};
use relm4::{gtk, prelude::*};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

fn pump(duration: Duration) {
    let until = Instant::now() + duration;
    let context = gtk::glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn get_rss_kb() -> usize {
    if let Ok(smaps) = std::fs::read_to_string("/proc/self/smaps_rollup") {
        for line in smaps.lines() {
            if line.starts_with("Rss:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse().unwrap_or(0);
                }
            }
        }
    }
    0
}

fn make_test_page(route: &PageRoute) -> PageWire {
    let mut page = PageWire::new(route.to_string(), format!("Page {route:?}"));
    let mut items = Vec::new();
    for i in 0..12 {
        let mut item = PageItemWire::new(format!("item-{i}"), format!("Item Title {i}"));
        item.subtitle = Some(format!("Artist {i}"));
        item.artwork = Some(Artwork {
            url: format!("https://example.com/art/item-{i}/600x600bb.jpg"),
            width: Some(600),
            height: Some(600),
        });
        item.open_route = Some(PageRoute::Album(format!("album-{i}")));
        items.push(item);
    }
    let section = PageSectionWire::new("shelf-1", Some("Featured".to_string()), items);
    page.sections.push(section);
    page
}

#[test]
#[ignore = "requires GTK display; run explicitly during memory acceptance"]
fn test_memory_plateau_under_navigation_and_playback_cycles() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();

    let path = std::env::temp_dir().join(format!("malus-mem-test-{}.sock", std::process::id()));
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();

    let _server = relm4::spawn(async move {
        let listener = tokio::net::UnixListener::from_std(listener).unwrap();
        while let Ok((mut stream, _)) = listener.accept().await {
            relm4::spawn(async move {
                let mut buffer = Vec::new();
                while let Ok(Some(frame)) = read_frame(&mut stream, &mut buffer, 1_048_576).await {
                    let request: ClientRequest = match decode_message(&frame) {
                        Ok(r) => r,
                        Err(_) => break,
                    };
                    let response = match request {
                        ClientRequest::GetStatus => ClientResponse::Status(PlayerStatus::default()),
                        ClientRequest::GetQueue => ClientResponse::Queue(Queue::default()),
                        ClientRequest::GetNavigation => ClientResponse::Navigation(
                            NavigationWire::new(PageRoute::Home, Vec::new()),
                        ),
                        ClientRequest::GetPage { ref route } => {
                            ClientResponse::Page(make_test_page(route))
                        }
                        _ => ClientResponse::Ok,
                    };
                    if write_message(&mut stream, &response).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    let client = MalusClient::new(path);
    let app_handle = MalusApp::builder().launch(client).detach();
    pump(Duration::from_millis(200));

    let initial_rss = get_rss_kb();
    println!(
        "Initial RSS: {initial_rss} kB ({:.1} MB)",
        initial_rss as f64 / 1024.0
    );

    // 1. Repeated Navigation: Home -> Albums -> Detail -> Home (10 cycles)
    println!("--- Testing Repeated Navigation (10 cycles) ---");
    let mut nav_samples = Vec::new();
    for cycle in 0..10 {
        app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Home)));
        pump(Duration::from_millis(60));

        app_handle.emit(AppInput::Navigate(AppDestination::Page(
            PageRoute::LibraryAlbums,
        )));
        pump(Duration::from_millis(60));

        app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Album(
            "test-album".into(),
        ))));
        pump(Duration::from_millis(60));

        app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Home)));
        pump(Duration::from_millis(60));

        let rss = get_rss_kb();
        nav_samples.push(rss);
        println!(
            "Cycle {cycle}: RSS = {rss} kB ({:.1} MB)",
            rss as f64 / 1024.0
        );
    }

    // Assert plateau between cycle 5 and cycle 9 (must not grow monotonically)
    let cycle5_rss = nav_samples[5];
    let cycle9_rss = nav_samples[9];
    let nav_delta = (cycle9_rss as f64 - cycle5_rss as f64) / 1024.0;
    println!("Navigation RSS delta (cycle 5 to 9): {nav_delta:.2} MB");
    assert!(
        nav_delta < 15.0,
        "Navigation RSS must plateau, but grew by {nav_delta:.2} MB between cycles 5 and 9"
    );

    // 2. 20 Track Changes in NowPlayingPage
    println!("--- Testing 20 Track Changes ---");
    let player = Rc::new(RefCell::new(PlayerPresentation::default()));
    let send: CommandHandler = Rc::new(|_| {});
    let menu: MenuHandler = Rc::new(|_| {});
    let now_playing = NowPlayingPage::new(&player, &send, &menu, || {}, || {}, || {});

    let mut track_samples = Vec::new();
    for i in 0..20 {
        let dummy_pixels = vec![((i * 12) % 255) as u8; 384 * 384 * 4];
        let bytes = relm4::gtk::glib::Bytes::from_owned(dummy_pixels);
        let img = DecodedImage {
            width: 384,
            height: 384,
            stride: 384 * 4,
            bytes,
        };
        now_playing.set_artwork(Some(&img));
        pump(Duration::from_millis(20));

        let rss = get_rss_kb();
        track_samples.push(rss);
    }

    let track10_rss = track_samples[10];
    let track19_rss = track_samples[19];
    let track_delta = (track19_rss as f64 - track10_rss as f64) / 1024.0;
    println!("Track change RSS delta (track 10 to 19): {track_delta:.2} MB");
    assert!(
        track_delta < 10.0,
        "Track changes RSS must plateau, but grew by {track_delta:.2} MB between tracks 10 and 19"
    );

    // 3. 10x Now Playing Open/Close Cycles
    println!("--- Testing 10x Now Playing Open/Close Cycles ---");
    let mut np_samples = Vec::new();
    for _cycle in 0..10 {
        app_handle.emit(AppInput::OpenNowPlaying(NowPlayingMode::Player));
        pump(Duration::from_millis(40));

        app_handle.emit(AppInput::CloseNowPlaying);
        pump(Duration::from_millis(40));

        let rss = get_rss_kb();
        np_samples.push(rss);
    }

    let np5_rss = np_samples[5];
    let np9_rss = np_samples[9];
    let np_delta = (np9_rss as f64 - np5_rss as f64) / 1024.0;
    println!("Now Playing open/close delta (cycle 5 to 9): {np_delta:.2} MB");
    assert!(
        np_delta < 10.0,
        "Now Playing open/close RSS must plateau, but grew by {np_delta:.2} MB"
    );

    let final_rss = get_rss_kb();
    println!(
        "Final RSS: {final_rss} kB ({:.1} MB)",
        final_rss as f64 / 1024.0
    );
    assert!(
        (final_rss as f64 / 1024.0) < 250.0,
        "Total RSS must remain under 250 MB, was {:.1} MB",
        final_rss as f64 / 1024.0
    );
}
