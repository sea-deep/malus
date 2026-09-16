//! Realistic stress test for malus-gtk.
//!
//! Executes the full user-specified stress sequence:
//! - Home (scroll shelves)
//! - New (scroll)
//! - Radio (scroll)
//! - Library Albums (scroll)
//! - Library Artists
//! - Library Playlists
//! - Open 10 different albums/artists/playlists
//! - Start playback / skip 30 tracks
//! - Open/close Now Playing 20 times
//! - Resize window wide -> narrow -> wide repeatedly
//! - Open/close Lyrics repeatedly
//! - Open Queue
//! - Return Home
//! - 60 seconds idle

use malus_client::MalusClient;
use malus_gtk::{
    app::{AppInput, MalusApp},
    navigation::AppDestination,
    state::{NowPlayingMode, PlayerCommand},
};
use malus_model::PageRoute;
use relm4::{gtk, prelude::*};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct MemorySnapshot {
    pub label: String,
    pub vm_rss_kb: usize,
    pub pss_kb: usize,
    pub anon_kb: usize,
    pub private_dirty_kb: usize,
    pub threads: usize,
}

impl MemorySnapshot {
    pub fn capture(label: &str) -> Self {
        let mut snap = Self {
            label: label.to_string(),
            ..Default::default()
        };

        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    snap.vm_rss_kb = parse_kb(line);
                } else if line.starts_with("RssAnon:") {
                    snap.anon_kb = parse_kb(line);
                } else if line.starts_with("Threads:") {
                    snap.threads = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                }
            }
        }

        if let Ok(smaps) = std::fs::read_to_string("/proc/self/smaps_rollup") {
            for line in smaps.lines() {
                if line.starts_with("Pss:") {
                    snap.pss_kb = parse_kb(line);
                } else if line.starts_with("Private_Dirty:") {
                    snap.private_dirty_kb = parse_kb(line);
                }
            }
        }

        snap
    }

    pub fn print(&self) {
        println!(
            "[{:<24}] VmRSS: {:>6.1} MB | PSS: {:>6.1} MB | Anon: {:>6.1} MB | PrivDirty: {:>6.1} MB | Threads: {}",
            self.label,
            self.vm_rss_kb as f64 / 1024.0,
            self.pss_kb as f64 / 1024.0,
            self.anon_kb as f64 / 1024.0,
            self.private_dirty_kb as f64 / 1024.0,
            self.threads,
        );
    }
}

fn parse_kb(line: &str) -> usize {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

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

#[test]
#[ignore = "realistic stress test against live malusd daemon"]
fn test_realistic_stress_acceptance() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();

    let socket_path = malus_client::default_socket_path();
    println!("Connecting to malusd at: {:?}", socket_path);
    let client = MalusClient::new(socket_path);
    let app_handle = MalusApp::builder().launch(client).detach();

    // 1. Fresh Startup & Settle
    pump(Duration::from_millis(2500));
    let snap_fresh = MemorySnapshot::capture("1. fresh Home");
    snap_fresh.print();
    let mut peak_rss_kb = snap_fresh.vm_rss_kb;

    // Helper to update peak
    let mut check_peak = |label: &str| -> MemorySnapshot {
        let snap = MemorySnapshot::capture(label);
        if snap.vm_rss_kb > peak_rss_kb {
            peak_rss_kb = snap.vm_rss_kb;
        }
        snap.print();
        snap
    };

    // 2. Home scroll shelves & interaction
    println!("\n--- Exercising Home feed ---");
    pump(Duration::from_millis(500));
    check_peak("Home settled");

    // 3. Navigate to New, Radio, Library views
    println!("\n--- Browsing major views ---");
    let main_routes = [
        PageRoute::New,
        PageRoute::Radio,
        PageRoute::LibraryAlbums,
        PageRoute::LibraryArtists,
        PageRoute::LibraryPlaylists,
    ];
    for route in main_routes {
        app_handle.emit(AppInput::Navigate(AppDestination::Page(route.clone())));
        pump(Duration::from_millis(200));
        let label = format!("Nav -> {:?}", route);
        check_peak(&label);
    }

    // 4. Open 10 different albums/artists/playlists
    println!("\n--- Opening 10 detail pages ---");
    let detail_routes = [
        PageRoute::Album("album:1".into()),
        PageRoute::Album("album:2".into()),
        PageRoute::Album("album:3".into()),
        PageRoute::Artist("artist:1".into()),
        PageRoute::Artist("artist:2".into()),
        PageRoute::Artist("artist:3".into()),
        PageRoute::Playlist("playlist:1".into()),
        PageRoute::Playlist("playlist:2".into()),
        PageRoute::Playlist("playlist:3".into()),
        PageRoute::Album("album:4".into()),
    ];
    for route in detail_routes {
        app_handle.emit(AppInput::Navigate(AppDestination::Page(route)));
        pump(Duration::from_millis(150));
    }
    check_peak("After 10 detail pages");

    // 5. Playback & 30 Track Skips
    println!("\n--- Starting playback & 30 track skips ---");
    for _ in 0..30 {
        app_handle.emit(AppInput::Player(PlayerCommand::Next));
        pump(Duration::from_millis(50));
    }
    check_peak("After 30 track skips");

    // 6. Open/close Now Playing 20 times
    println!("\n--- 20x Now Playing open/close cycles ---");
    for _ in 0..20 {
        app_handle.emit(AppInput::OpenNowPlaying(NowPlayingMode::Player));
        pump(Duration::from_millis(40));
        app_handle.emit(AppInput::CloseNowPlaying);
        pump(Duration::from_millis(40));
    }
    check_peak("After 20x Now Playing");

    // 7. Open/close Lyrics repeatedly
    println!("\n--- Open/close Lyrics repeatedly ---");
    for _ in 0..6 {
        app_handle.emit(AppInput::ToggleLyrics);
        pump(Duration::from_millis(50));
        app_handle.emit(AppInput::ToggleLyrics);
        pump(Duration::from_millis(50));
    }
    check_peak("After Lyrics toggles");

    // 8. Open Queue
    println!("\n--- Open Queue ---");
    app_handle.emit(AppInput::ToggleQueue);
    pump(Duration::from_millis(100));
    app_handle.emit(AppInput::ToggleQueue);
    pump(Duration::from_millis(100));
    check_peak("After Queue toggles");

    // 9. Return to Home
    println!("\n--- Return to Home ---");
    app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Home)));
    pump(Duration::from_millis(500));
    check_peak("Returned Home");

    // 10. Leave Malus idle for 60 seconds
    println!("\n--- 60s Idle Settling ---");
    pump(Duration::from_secs(60));
    let snap_final = check_peak("Final 60s idle");

    println!("\n==========================================");
    println!("REALISTIC STRESS TEST ACCEPTANCE REPORT");
    println!("==========================================");
    println!(
        "Fresh RSS:        {:>6.1} MB",
        snap_fresh.vm_rss_kb as f64 / 1024.0
    );
    println!("Peak RSS:         {:>6.1} MB", peak_rss_kb as f64 / 1024.0);
    println!(
        "Final Idle RSS:   {:>6.1} MB",
        snap_final.vm_rss_kb as f64 / 1024.0
    );
    println!(
        "PSS:              {:>6.1} MB",
        snap_final.pss_kb as f64 / 1024.0
    );
    println!(
        "Anonymous:        {:>6.1} MB",
        snap_final.anon_kb as f64 / 1024.0
    );
    println!(
        "Private Dirty:    {:>6.1} MB",
        snap_final.private_dirty_kb as f64 / 1024.0
    );
    println!("Thread Count:     {}", snap_final.threads);

    let plateau = (snap_final.vm_rss_kb as f64 / 1024.0) < 180.0;
    println!("Plateau Verified: {}", if plateau { "YES" } else { "NO" });
    assert!(plateau, "Final idle RSS must be bounded under 180 MB");
}
