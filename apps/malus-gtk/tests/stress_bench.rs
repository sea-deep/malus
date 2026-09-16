//! Stress bench and latency measurement for malus-gtk.
//!
//! Performs the exact interaction sequence:
//! - Startup / Home fully loaded
//! - 20 route transitions (Home -> Library Albums -> Album -> Home -> New -> Radio -> Library Artists -> Library Playlists)
//! - 10x Now Playing open/close
//! - 20 track skips / changes
//! - 30s idle
//!
//! Measures: VmRSS, PSS, Anonymous, Private_Dirty, Threads, and route latency.

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
            "[{:<22}] VmRSS: {:>6.1} MB | PSS: {:>6.1} MB | Anon: {:>6.1} MB | PrivDirty: {:>6.1} MB | Threads: {}",
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
#[ignore = "stress benchmark against live malusd daemon"]
fn test_stress_bench_interaction_sequence() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();

    let socket_path = malus_client::default_socket_path();
    println!("Connecting to malusd at: {:?}", socket_path);
    let client = MalusClient::new(socket_path);
    let app_handle = MalusApp::builder().launch(client).detach();

    // 1. Startup / Home settling
    pump(Duration::from_millis(2500));
    let snap_startup = MemorySnapshot::capture("startup");
    snap_startup.print();

    // Latency tests: measure route-switch times
    println!("\n=== MEASURING ROUTE-SWITCH LATENCY ===");

    // Home -> Album
    let t0 = Instant::now();
    app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Album(
        "album:1".into(),
    ))));
    pump(Duration::from_millis(150));
    let d_home_to_album = t0.elapsed();
    println!("Latency: Home -> Album: {:?}", d_home_to_album);

    // Album -> Home
    let t0 = Instant::now();
    app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Home)));
    pump(Duration::from_millis(150));
    let d_album_to_home = t0.elapsed();
    println!("Latency: Album -> Home: {:?}", d_album_to_home);

    // Library Albums -> Artist
    app_handle.emit(AppInput::Navigate(AppDestination::Page(
        PageRoute::LibraryAlbums,
    )));
    pump(Duration::from_millis(150));
    let t0 = Instant::now();
    app_handle.emit(AppInput::Navigate(AppDestination::Page(PageRoute::Artist(
        "artist:1".into(),
    ))));
    pump(Duration::from_millis(150));
    let d_lib_to_artist = t0.elapsed();
    println!("Latency: Library Albums -> Artist: {:?}", d_lib_to_artist);

    // Opening Now Playing
    let t0 = Instant::now();
    app_handle.emit(AppInput::OpenNowPlaying(NowPlayingMode::Player));
    pump(Duration::from_millis(50));
    let d_open_np = t0.elapsed();
    println!("Latency: Open Now Playing: {:?}", d_open_np);

    app_handle.emit(AppInput::CloseNowPlaying);
    pump(Duration::from_millis(50));

    // 2. 20 Route Transitions
    println!("\n=== EXECUTING 20 ROUTE TRANSITIONS ===");
    let routes = [
        PageRoute::Home,
        PageRoute::LibraryAlbums,
        PageRoute::Album("album:1".into()),
        PageRoute::Home,
        PageRoute::New,
        PageRoute::Radio,
        PageRoute::LibraryArtists,
        PageRoute::LibraryPlaylists,
    ];

    let mut snap_5 = None;
    let mut snap_10 = None;
    let mut snap_20 = None;

    for i in 1..=20 {
        let route = routes[(i - 1) % routes.len()].clone();
        app_handle.emit(AppInput::Navigate(AppDestination::Page(route)));
        pump(Duration::from_millis(100));

        if i == 5 {
            let snap = MemorySnapshot::capture("5 transitions");
            snap.print();
            snap_5 = Some(snap);
        } else if i == 10 {
            let snap = MemorySnapshot::capture("10 transitions");
            snap.print();
            snap_10 = Some(snap);
        } else if i == 20 {
            let snap = MemorySnapshot::capture("20 transitions");
            snap.print();
            snap_20 = Some(snap);
        }
    }

    // 3. 10x Now Playing Cycles
    println!("\n=== EXECUTING 10x NOW PLAYING CYCLES ===");
    for _ in 0..10 {
        app_handle.emit(AppInput::OpenNowPlaying(NowPlayingMode::Player));
        pump(Duration::from_millis(50));
        app_handle.emit(AppInput::CloseNowPlaying);
        pump(Duration::from_millis(50));
    }
    let snap_np = MemorySnapshot::capture("Now Playing 10x");
    snap_np.print();

    // 4. 20 Track Skips
    println!("\n=== EXECUTING 20 TRACK SKIPS ===");
    for _ in 0..20 {
        app_handle.emit(AppInput::Player(PlayerCommand::Next));
        pump(Duration::from_millis(50));
    }
    let snap_tracks = MemorySnapshot::capture("20 track skips");
    snap_tracks.print();

    // 5. Final 30s Idle
    println!("\n=== 30s IDLE SETTLING ===");
    pump(Duration::from_secs(30));
    let snap_final = MemorySnapshot::capture("final 30s idle");
    snap_final.print();

    println!("\n=== SUMMARY TELEMETRY ===");
    println!(
        "Startup RSS:      {:.1} MB",
        snap_startup.vm_rss_kb as f64 / 1024.0
    );
    if let Some(s) = snap_5 {
        println!("5 Transitions:    {:.1} MB", s.vm_rss_kb as f64 / 1024.0);
    }
    if let Some(s) = snap_10 {
        println!("10 Transitions:   {:.1} MB", s.vm_rss_kb as f64 / 1024.0);
    }
    if let Some(s) = snap_20 {
        println!("20 Transitions:   {:.1} MB", s.vm_rss_kb as f64 / 1024.0);
    }
    println!(
        "Now Playing 10x:  {:.1} MB",
        snap_np.vm_rss_kb as f64 / 1024.0
    );
    println!(
        "Track Skips 20:   {:.1} MB",
        snap_tracks.vm_rss_kb as f64 / 1024.0
    );
    println!(
        "Final 30s Idle:   {:.1} MB",
        snap_final.vm_rss_kb as f64 / 1024.0
    );
}
