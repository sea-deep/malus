//! Malus Command Line Interface (`malusctl`).

use clap::{Parser, Subcommand};
use malus_client::{MalusClient, default_socket_path};
use malus_ipc::{
    client::{ClientRequest, ClientResponse},
    wire::{
        CatalogItemWire, LibraryKindWire, LibraryPageWire, NavigationWire, PageWire,
        SearchKindWire, SearchResultsWire,
    },
};
use malus_model::{MediaRef, PageRoute, Queue};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "malusctl", about = "Malus Apple Music player CLI")]
struct Cli {
    /// Path to malus daemon UNIX domain socket
    #[arg(short, long, global = true)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum LibraryCommands {
    /// Browse saved library tracks
    #[command(alias = "track")]
    Tracks {
        /// Maximum number of tracks to fetch
        #[arg(short = 'l', long = "limit")]
        limit: Option<usize>,

        /// Opaque pagination cursor
        #[arg(short = 'c', long = "cursor")]
        cursor: Option<String>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Browse saved library albums
    #[command(alias = "album")]
    Albums {
        /// Maximum number of albums to fetch
        #[arg(short = 'l', long = "limit")]
        limit: Option<usize>,

        /// Opaque pagination cursor
        #[arg(short = 'c', long = "cursor")]
        cursor: Option<String>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Browse saved library playlists
    #[command(alias = "playlist")]
    Playlists {
        /// Maximum number of playlists to fetch
        #[arg(short = 'l', long = "limit")]
        limit: Option<usize>,

        /// Opaque pagination cursor
        #[arg(short = 'c', long = "cursor")]
        cursor: Option<String>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum QueueCommands {
    /// Add media to play next in queue
    Next {
        #[arg(help = "Media ID (e.g. song:123456, album:7890)")]
        media_id: String,
    },
    /// Add media to play later in queue
    Later {
        #[arg(help = "Media ID (e.g. song:123456, album:7890)")]
        media_id: String,
    },
    /// Jump to a specific queue index
    Jump {
        #[arg(help = "Target queue index (0-based)")]
        index: usize,
    },
    /// Remove an item at the specified index from the queue
    Remove {
        #[arg(help = "Queue index to remove (0-based)")]
        index: usize,
    },
    /// Move a queue item from one index to another
    Move {
        #[arg(help = "Source queue index (0-based)")]
        from: usize,
        #[arg(help = "Destination queue index (0-based)")]
        to: usize,
    },
    /// Clear all upcoming items in the queue (preserving current)
    #[command(name = "clear-upcoming")]
    ClearUpcoming,
}

#[derive(Subcommand)]
enum Commands {
    /// Browse navigation tree (Discover, Library, Replay)
    Nav,

    /// Inspect a product page feed (home, new, radio, album:<id>, etc.)
    Page {
        /// Page route (e.g. home, new, radio, album:<id>, playlist:<id>, artist:<id>, replay:2024, library:songs)
        route: String,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Initiate interactive authentication for Apple Music
    Login,

    /// Log out from Apple Music and clear session data
    Logout,

    /// Inspect Apple Music authentication status
    AuthStatus,

    /// Resume playback, or play a specified media ID (e.g. song:1440857781)
    #[command(alias = "resume")]
    Play {
        #[arg(help = "Optional media ID to play (e.g. song:1440857781)")]
        media_id: Option<String>,
    },

    /// Pause playback
    Pause,

    /// Toggle playback state between play and pause
    Toggle,

    /// Stop playback and reset position
    Stop,

    /// Skip to next track in queue
    Next,

    /// Return to previous track in queue
    Prev,

    /// Seek to a specific position (seconds or mm:ss) or relative offset (+15, -10)
    Seek {
        #[arg(help = "Target position (e.g. 90, 1:30) or relative offset (e.g. +15, -10)")]
        target: String,
    },

    /// Set volume (0 - 100)
    Volume {
        #[arg(help = "Volume level from 0 to 100")]
        volume: u8,
    },

    /// Display current player status
    Status,

    /// Watch live player events
    Watch {
        /// Output events as raw JSON lines
        #[arg(long)]
        json: bool,
    },

    /// Display or manipulate the playback queue
    Queue {
        #[command(subcommand)]
        action: Option<QueueCommands>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Search Apple Music audio catalog
    Search {
        #[arg(help = "Search query string")]
        query: String,

        /// Filter by media type (track, album, artist, playlist)
        #[arg(short = 't', long = "type")]
        r#type: Option<String>,

        /// Maximum number of results per category
        #[arg(short = 'l', long = "limit")]
        limit: Option<usize>,

        /// Opaque pagination cursor
        #[arg(short = 'c', long = "cursor")]
        cursor: Option<String>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect catalog track details
    Track {
        #[arg(help = "Track media ID (e.g. song:1440857781)")]
        media_id: String,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect catalog album details and tracklist
    Album {
        #[arg(help = "Album media ID (e.g. album:1440857780)")]
        media_id: String,

        /// Maximum number of collection tracks to fetch
        #[arg(short = 'l', long = "limit")]
        limit: Option<usize>,

        /// Opaque pagination cursor for tracklist
        #[arg(short = 'c', long = "cursor")]
        cursor: Option<String>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect catalog artist details
    Artist {
        #[arg(help = "Artist media ID (e.g. artist:5468295)")]
        media_id: String,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect catalog playlist details and tracklist
    Playlist {
        #[arg(help = "Playlist media ID (e.g. playlist:pl.xyz)")]
        media_id: String,

        /// Maximum number of collection tracks to fetch
        #[arg(short = 'l', long = "limit")]
        limit: Option<usize>,

        /// Opaque pagination cursor for tracklist
        #[arg(short = 'c', long = "cursor")]
        cursor: Option<String>,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Browse user's saved library
    Library {
        #[command(subcommand)]
        action: LibraryCommands,
    },

    /// Stream live player events
    Events,

    /// Ping the daemon to check connectivity
    Ping,

    /// Run system diagnostics and verify Malus prerequisites
    Doctor,

    /// Configure, install, or verify Widevine CDM
    SetupWidevine {
        /// Optional path to libwidevinecdm.so or WidevineCdm directory
        #[arg(long, help = "Path to libwidevinecdm.so or WidevineCdm directory")]
        path: Option<PathBuf>,

        /// Download and install Widevine CDM from Google's official Linux package
        #[arg(
            long,
            help = "Download and install Widevine CDM from Google's official package"
        )]
        install: bool,

        /// Accept Google Chrome terms of service (required for non-interactive --install)
        #[arg(long, help = "Accept Google Chrome terms of service for --install")]
        accept_google_terms: bool,

        /// Force re-installation of managed Widevine CDM even if already present
        #[arg(long, help = "Force reinstall of managed Widevine CDM")]
        force: bool,

        /// Reset Widevine configuration and delete Malus-managed CDM files
        #[arg(
            long,
            help = "Reset Widevine configuration and delete Malus-managed files"
        )]
        reset: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Standalone commands that do not require an active daemon connection
    match cli.command {
        Commands::Doctor => {
            run_doctor();
            return Ok(());
        }
        Commands::SetupWidevine {
            path,
            install,
            accept_google_terms,
            force,
            reset,
        } => {
            run_setup_widevine(path, install, accept_google_terms, force, reset).await?;
            return Ok(());
        }
        _ => {}
    }

    let socket_path = cli.socket.unwrap_or_else(default_socket_path);

    let mut client = MalusClient::connect(&socket_path).await.map_err(|e| {
        eprintln!("Error: Cannot connect to malus daemon: {e}");
        eprintln!(
            "Ensure 'malusd' is running and socket exists at {}",
            socket_path.display()
        );
        e
    })?;

    match cli.command {
        Commands::Nav => {
            let resp = client.send(&ClientRequest::GetNavigation).await?;
            match resp {
                ClientResponse::Navigation(nav) => {
                    display_navigation(&nav);
                }
                other => print_response(&other),
            }
        }
        Commands::Page { route, json } => {
            let parsed_route = match route.parse::<PageRoute>() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid route '{route}': {e}");
                    return Ok(());
                }
            };
            let resp = client
                .send(&ClientRequest::GetPage {
                    route: parsed_route,
                })
                .await?;
            match resp {
                ClientResponse::Page(page) => {
                    display_page(&page, json)?;
                }
                ClientResponse::Error { code, message } => {
                    eprintln!("Error [{code}]: {message}");
                }
                other => print_response(&other),
            }
        }
        Commands::Login => {
            println!("Initiating Apple Music login...");
            let resp = client.send(&ClientRequest::AuthBegin).await?;
            match resp {
                ClientResponse::AuthStatus(s) => {
                    println!("Status: {}", s.state);
                    if let Some(msg) = s.message {
                        println!("Message: {msg}");
                    }
                }
                other => print_response(&other),
            }
        }
        Commands::Logout => {
            println!("Logging out from Apple Music...");
            let resp = client.send(&ClientRequest::AuthLogout).await?;
            match resp {
                ClientResponse::AuthStatus(s) => {
                    println!("Status: {}", s.state);
                    if let Some(msg) = s.message {
                        println!("Message: {msg}");
                    }
                }
                other => print_response(&other),
            }
        }
        Commands::AuthStatus => {
            let resp = client.send(&ClientRequest::GetAuthStatus).await?;
            match resp {
                ClientResponse::AuthStatus(s) => {
                    println!("Auth Status: {}", s.state);
                    if let Some(msg) = s.message {
                        println!("Message:     {msg}");
                    }
                }
                other => print_response(&other),
            }
        }
        Commands::Play { media_id } => {
            let req = match media_id {
                Some(id) => match MediaRef::parse(&id) {
                    Ok(reference) => ClientRequest::PlayMedia { reference },
                    Err(e) => {
                        eprintln!("Invalid media reference '{id}': {e}");
                        return Ok(());
                    }
                },
                None => ClientRequest::Play,
            };
            let resp = client.send(&req).await?;
            print_response(&resp);
        }
        Commands::Pause => {
            let resp = client.send(&ClientRequest::Pause).await?;
            print_response(&resp);
        }
        Commands::Toggle => {
            let resp = client.send(&ClientRequest::TogglePlay).await?;
            print_response(&resp);
        }
        Commands::Stop => {
            let resp = client.send(&ClientRequest::Stop).await?;
            print_response(&resp);
        }
        Commands::Next => {
            let resp = client.send(&ClientRequest::Next).await?;
            print_response(&resp);
        }
        Commands::Prev => {
            let resp = client.send(&ClientRequest::Previous).await?;
            print_response(&resp);
        }
        Commands::Seek { target } => {
            let position_ms = resolve_seek_target(&mut client, &target).await?;
            let resp = client.send(&ClientRequest::Seek { position_ms }).await?;
            print_response(&resp);
        }
        Commands::Volume { volume } => {
            let resp = client.send(&ClientRequest::SetVolume { volume }).await?;
            print_response(&resp);
        }
        Commands::Status => {
            let resp = client.send(&ClientRequest::GetStatus).await?;
            match resp {
                ClientResponse::Status(s) => {
                    let track_info = match s.current_track {
                        Some(t) => format!("{} - {}", t.title, t.artist_display()),
                        None => "(none)".to_string(),
                    };
                    println!("State:      {:?}", s.state);
                    println!("Track:      {}", track_info);
                    println!(
                        "Position:   {:.1}s / {:.1}s",
                        s.position_ms as f64 / 1000.0,
                        s.duration_ms as f64 / 1000.0
                    );
                    println!(
                        "Volume:     {}%{}",
                        s.volume,
                        if s.muted { " [MUTED]" } else { "" }
                    );
                    println!("Shuffle:    {}", s.shuffle);
                    println!("Repeat:     {:?}", s.repeat);
                }
                other => print_response(&other),
            }
        }
        Commands::Queue { action, json } => match action {
            None => {
                let resp = client.send(&ClientRequest::GetQueue).await?;
                match resp {
                    ClientResponse::Queue(q) => {
                        display_queue(&q, json)?;
                    }
                    other => print_response(&other),
                }
            }
            Some(QueueCommands::Next { media_id }) => {
                let mref = MediaRef::parse(&media_id)?;
                let resp = client
                    .send(&ClientRequest::PlayNext { reference: mref })
                    .await?;
                match resp {
                    ClientResponse::Ok => {
                        let q_resp = client.send(&ClientRequest::GetQueue).await?;
                        if let ClientResponse::Queue(q) = q_resp {
                            display_queue(&q, json)?;
                        } else if json {
                            println!("{{\"status\":\"ok\"}}");
                        } else {
                            println!("Item queued to play next.");
                        }
                    }
                    other => print_response(&other),
                }
            }
            Some(QueueCommands::Later { media_id }) => {
                let mref = MediaRef::parse(&media_id)?;
                let resp = client
                    .send(&ClientRequest::PlayLater { reference: mref })
                    .await?;
                match resp {
                    ClientResponse::Ok => {
                        let q_resp = client.send(&ClientRequest::GetQueue).await?;
                        if let ClientResponse::Queue(q) = q_resp {
                            display_queue(&q, json)?;
                        } else if json {
                            println!("{{\"status\":\"ok\"}}");
                        } else {
                            println!("Item queued to play later.");
                        }
                    }
                    other => print_response(&other),
                }
            }
            Some(QueueCommands::Jump { index }) => {
                let resp = client.send(&ClientRequest::QueueJump { index }).await?;
                match resp {
                    ClientResponse::Ok => {
                        let q_resp = client.send(&ClientRequest::GetQueue).await?;
                        if let ClientResponse::Queue(q) = q_resp {
                            display_queue(&q, json)?;
                        } else if json {
                            println!("{{\"status\":\"ok\"}}");
                        } else {
                            println!("Jumped to queue index {index}.");
                        }
                    }
                    other => print_response(&other),
                }
            }
            Some(QueueCommands::Remove { index }) => {
                let resp = client.send(&ClientRequest::QueueRemove { index }).await?;
                match resp {
                    ClientResponse::Ok => {
                        let q_resp = client.send(&ClientRequest::GetQueue).await?;
                        if let ClientResponse::Queue(q) = q_resp {
                            display_queue(&q, json)?;
                        } else if json {
                            println!("{{\"status\":\"ok\"}}");
                        } else {
                            println!("Removed item at index {index}.");
                        }
                    }
                    other => print_response(&other),
                }
            }
            Some(QueueCommands::Move { from, to }) => {
                let resp = client.send(&ClientRequest::QueueMove { from, to }).await?;
                match resp {
                    ClientResponse::Ok => {
                        let q_resp = client.send(&ClientRequest::GetQueue).await?;
                        if let ClientResponse::Queue(q) = q_resp {
                            display_queue(&q, json)?;
                        } else if json {
                            println!("{{\"status\":\"ok\"}}");
                        } else {
                            println!("Moved item from index {from} to {to}.");
                        }
                    }
                    other => print_response(&other),
                }
            }
            Some(QueueCommands::ClearUpcoming) => {
                let resp = client.send(&ClientRequest::QueueClearUpcoming).await?;
                match resp {
                    ClientResponse::Ok => {
                        let q_resp = client.send(&ClientRequest::GetQueue).await?;
                        if let ClientResponse::Queue(q) = q_resp {
                            display_queue(&q, json)?;
                        } else if json {
                            println!("{{\"status\":\"ok\"}}");
                        } else {
                            println!("Cleared upcoming queue items.");
                        }
                    }
                    other => print_response(&other),
                }
            }
        },
        Commands::Search {
            query,
            r#type,
            limit,
            cursor,
            json,
        } => {
            let kinds = match r#type.as_deref() {
                Some("track" | "tracks" | "song" | "songs") => vec![SearchKindWire::Track],
                Some("album" | "albums") => vec![SearchKindWire::Album],
                Some("artist" | "artists") => vec![SearchKindWire::Artist],
                Some("playlist" | "playlists") => vec![SearchKindWire::Playlist],
                Some(unknown) => {
                    eprintln!(
                        "Unknown type '{unknown}'. Valid types: track, album, artist, playlist"
                    );
                    return Ok(());
                }
                None => vec![],
            };

            let resp = client
                .send(&ClientRequest::Search {
                    query,
                    kinds,
                    limit,
                    cursor,
                })
                .await?;

            match resp {
                ClientResponse::SearchResults(results) => {
                    display_search_results(&results, json)?;
                }
                ClientResponse::Error { code, message } => {
                    eprintln!("Error [{code}]: {message}");
                }
                other => print_response(&other),
            }
        }
        Commands::Track { media_id, json } => {
            let reference = match MediaRef::parse(&media_id) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid media reference '{media_id}': {e}");
                    return Ok(());
                }
            };
            let resp = client
                .send(&ClientRequest::GetCatalogItem { reference })
                .await?;
            match resp {
                ClientResponse::CatalogItem(item) => {
                    display_track(&item, json)?;
                }
                ClientResponse::Error { code, message } => {
                    eprintln!("Error [{code}]: {message}");
                }
                other => print_response(&other),
            }
        }
        Commands::Album {
            media_id,
            limit,
            cursor,
            json,
        } => {
            let reference = match MediaRef::parse(&media_id) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid media reference '{media_id}': {e}");
                    return Ok(());
                }
            };
            let item_resp = client
                .send(&ClientRequest::GetCatalogItem {
                    reference: reference.clone(),
                })
                .await?;
            let tracks_resp = client
                .send(&ClientRequest::GetCollectionItems {
                    reference,
                    limit,
                    cursor,
                })
                .await?;
            display_album(&item_resp, &tracks_resp, json)?;
        }
        Commands::Artist { media_id, json } => {
            let reference = match MediaRef::parse(&media_id) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid media reference '{media_id}': {e}");
                    return Ok(());
                }
            };
            let resp = client
                .send(&ClientRequest::GetCatalogItem { reference })
                .await?;
            match resp {
                ClientResponse::CatalogItem(item) => {
                    display_artist(&item, json)?;
                }
                ClientResponse::Error { code, message } => {
                    eprintln!("Error [{code}]: {message}");
                }
                other => print_response(&other),
            }
        }
        Commands::Playlist {
            media_id,
            limit,
            cursor,
            json,
        } => {
            let reference = match MediaRef::parse(&media_id) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid media reference '{media_id}': {e}");
                    return Ok(());
                }
            };
            let item_resp = client
                .send(&ClientRequest::GetCatalogItem {
                    reference: reference.clone(),
                })
                .await?;
            let tracks_resp = client
                .send(&ClientRequest::GetCollectionItems {
                    reference,
                    limit,
                    cursor,
                })
                .await?;
            display_playlist(&item_resp, &tracks_resp, json)?;
        }
        Commands::Library { action } => match action {
            LibraryCommands::Tracks {
                limit,
                cursor,
                json,
            } => {
                let resp = client
                    .send(&ClientRequest::GetLibrary {
                        kind: LibraryKindWire::Tracks,
                        limit,
                        cursor,
                    })
                    .await?;
                display_library(&resp, json)?;
            }
            LibraryCommands::Albums {
                limit,
                cursor,
                json,
            } => {
                let resp = client
                    .send(&ClientRequest::GetLibrary {
                        kind: LibraryKindWire::Albums,
                        limit,
                        cursor,
                    })
                    .await?;
                display_library(&resp, json)?;
            }
            LibraryCommands::Playlists {
                limit,
                cursor,
                json,
            } => {
                let resp = client
                    .send(&ClientRequest::GetLibrary {
                        kind: LibraryKindWire::Playlists,
                        limit,
                        cursor,
                    })
                    .await?;
                display_library(&resp, json)?;
            }
        },
        Commands::Watch { json } => {
            if !json {
                println!("Subscribing to live player events (Ctrl+C to stop)...");
            }
            client
                .stream_events(|event| {
                    if json {
                        if let Ok(serialized) = serde_json::to_string(&event) {
                            println!("{serialized}");
                        }
                    } else {
                        match &event {
                            malus_ipc::client::ClientEvent::StatusChanged(s) => {
                                let track_str = match &s.current_track {
                                    Some(t) => format!("{} - {}", t.title, t.artist_display()),
                                    None => "(none)".to_string(),
                                };
                                println!(
                                    "[{:?}] {} ({:.1}s / {:.1}s)",
                                    s.state,
                                    track_str,
                                    s.position_ms as f64 / 1000.0,
                                    s.duration_ms as f64 / 1000.0
                                );
                            }
                            malus_ipc::client::ClientEvent::TrackChanged(track) => {
                                let track_str = match track {
                                    Some(t) => format!("{} - {}", t.title, t.artist_display()),
                                    None => "(none)".to_string(),
                                };
                                println!("[Track] {track_str}");
                            }
                            malus_ipc::client::ClientEvent::AuthChanged(auth) => {
                                println!("[Auth] {}", auth.state);
                            }
                            malus_ipc::client::ClientEvent::QueueChanged(q) => {
                                println!("[Queue] {} tracks", q.items.len());
                            }
                        }
                    }
                })
                .await?;
        }
        Commands::Events => {
            println!("Subscribing to live daemon events (Ctrl+C to stop)...");
            client
                .stream_events(|event| {
                    println!("[Event] {event:?}");
                })
                .await?;
        }
        Commands::Ping => {
            let resp = client.send(&ClientRequest::Ping).await?;
            match resp {
                ClientResponse::Pong => println!("pong"),
                other => print_response(&other),
            }
        }
        Commands::Doctor | Commands::SetupWidevine { .. } => unreachable!(),
    }

    Ok(())
}

fn format_duration_ms(ms: Option<u64>) -> String {
    match ms {
        Some(ms) => {
            let total_secs = ms / 1000;
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            format!("{mins:02}:{secs:02}")
        }
        None => "--:--".to_string(),
    }
}

fn display_queue(queue: &Queue, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(queue)?);
        return Ok(());
    }

    if queue.items.is_empty() {
        println!("Queue is empty.");
        return Ok(());
    }

    println!("Queue\n");
    for (idx, track) in queue.items.iter().enumerate() {
        let marker = if queue.current_index == Some(idx) {
            ">"
        } else {
            " "
        };
        let artist = track.artist_display();
        println!("{marker} {:2}  {:<26} {:<24}", idx, track.title, artist);
    }
    Ok(())
}

fn display_navigation(nav: &NavigationWire) {
    println!("Apple Music Navigation\n");
    for group in &nav.groups {
        let title = group.title.as_deref().unwrap_or(&group.id);
        println!("{}", title.to_uppercase());
        for entry in &group.entries {
            let icon = entry
                .icon_hint
                .as_deref()
                .map(|i| format!(" [{i}]"))
                .unwrap_or_default();
            println!("  {:<25} -> {}{}", entry.label, entry.route, icon);
        }
        println!();
    }
}

fn display_page(page: &PageWire, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(page)?);
        return Ok(());
    }

    println!("{} ({})", page.title, page.id);
    if let Some(ref sub) = page.subtitle {
        println!("  {sub}");
    }
    if let Some(ref hdr) = page.header {
        println!("\n  Header: {}", hdr.title);
        if let Some(ref sub) = hdr.subtitle {
            println!("  Subtitle: {sub}");
        }
        if !hdr.metadata.is_empty() {
            println!("  Metadata: {}", hdr.metadata.join(" • "));
        }
    }

    println!();
    for (sec_idx, sec) in page.sections.iter().enumerate() {
        let sec_title = sec.title.as_deref().unwrap_or("Section");
        let hint = sec.presentation_hint.as_deref().unwrap_or("default");
        println!("--- [{}] {} ({hint}) ---", sec_idx + 1, sec_title);
        if let Some(ref sub) = sec.subtitle {
            println!("    {sub}");
        }
        if sec.items.is_empty() {
            println!("    (no items)");
        } else {
            for (idx, item) in sec.items.iter().enumerate() {
                let sub = item.subtitle.as_deref().unwrap_or("-");
                let dest = item
                    .open_route
                    .as_ref()
                    .map(|r| format!(" -> {r}"))
                    .unwrap_or_default();
                let badges = if !item.badges.is_empty() {
                    let badge_str = item
                        .badges
                        .iter()
                        .map(|b| b.label.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(" [{badge_str}]")
                } else {
                    String::new()
                };
                println!(
                    "    {:2}. [{}] {} - {}{}{}",
                    idx + 1,
                    item.id,
                    item.title,
                    sub,
                    badges,
                    dest
                );
            }
        }
        if let Some(ref cont) = sec.continuation {
            println!("    Continuation token: {}", cont.token);
        }
        println!();
    }

    if let Some(ref cont) = page.continuation {
        println!("Page continuation token: {}", cont.token);
    }

    Ok(())
}

fn display_search_results(
    results: &SearchResultsWire,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(results)?);
        return Ok(());
    }

    let mut found_any = false;

    if let Some(ref tracks) = results.tracks
        && !tracks.items.is_empty()
    {
        found_any = true;
        println!("TRACKS ({}):", tracks.items.len());
        for (idx, t) in tracks.items.iter().enumerate() {
            let alb = t.album_title().unwrap_or("-");
            let dur = format_duration_ms(t.duration_ms);
            println!(
                "  {:2}. [{}] {} - {} ({}) [{}]",
                idx + 1,
                t.id,
                t.title,
                t.artist_display(),
                alb,
                dur
            );
        }
        if let Some(ref cur) = tracks.next_cursor {
            println!("  Next cursor (tracks): {cur}");
        }
        println!();
    }

    if let Some(ref albums) = results.albums
        && !albums.items.is_empty()
    {
        found_any = true;
        println!("ALBUMS ({}):", albums.items.len());
        for (idx, a) in albums.items.iter().enumerate() {
            let artists = a
                .artists
                .iter()
                .map(|art| art.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let count = a
                .track_count
                .map(|c| format!("{c} tracks"))
                .unwrap_or_else(|| "-".to_string());
            let year = a.release_date.as_deref().unwrap_or("-");
            println!(
                "  {:2}. [{}] {} - {} ({year}) [{count}]",
                idx + 1,
                a.id,
                a.title,
                artists
            );
        }
        if let Some(ref cur) = albums.next_cursor {
            println!("  Next cursor (albums): {cur}");
        }
        println!();
    }

    if let Some(ref artists) = results.artists
        && !artists.items.is_empty()
    {
        found_any = true;
        println!("ARTISTS ({}):", artists.items.len());
        for (idx, a) in artists.items.iter().enumerate() {
            println!("  {:2}. [{}] {}", idx + 1, a.id, a.name);
        }
        if let Some(ref cur) = artists.next_cursor {
            println!("  Next cursor (artists): {cur}");
        }
        println!();
    }

    if let Some(ref playlists) = results.playlists
        && !playlists.items.is_empty()
    {
        found_any = true;
        println!("PLAYLISTS ({}):", playlists.items.len());
        for (idx, p) in playlists.items.iter().enumerate() {
            let curator = p.curator.as_deref().unwrap_or("-");
            let count = p
                .track_count
                .map(|c| format!("{c} tracks"))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "  {:2}. [{}] {} [by {curator}] [{count}]",
                idx + 1,
                p.id,
                p.title
            );
        }
        if let Some(ref cur) = playlists.next_cursor {
            println!("  Next cursor (playlists): {cur}");
        }
        println!();
    }

    if !found_any {
        println!("No results found.");
    }

    Ok(())
}

fn display_track(item: &CatalogItemWire, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(item)?);
        return Ok(());
    }

    match item {
        CatalogItemWire::Track(t) => {
            println!("Track:       {}", t.id);
            println!("Title:       {}", t.title);
            println!("Artist:      {}", t.artist_display());
            if let Some(ref alb) = t.album {
                if let Some(ref id) = alb.id {
                    println!("Album:       {} ({id})", alb.title);
                } else {
                    println!("Album:       {}", alb.title);
                }
            }
            println!("Duration:    {}", format_duration_ms(t.duration_ms));
            if let Some(track_num) = t.track_number {
                if let Some(disc) = t.disc_number {
                    println!("Track #:     {track_num} (disc {disc})");
                } else {
                    println!("Track #:     {track_num}");
                }
            }
            if let Some(exp) = t.explicit {
                println!("Explicit:    {}", if exp { "yes" } else { "no" });
            }
            if let Some(ref art) = t.artwork {
                println!("Artwork:     {}", art.url);
            }
        }
        other => {
            println!("Expected track item, received: {other:?}");
        }
    }
    Ok(())
}

fn display_album(
    item_resp: &ClientResponse,
    tracks_resp: &ClientResponse,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let mut obj = serde_json::Map::new();
        match item_resp {
            ClientResponse::CatalogItem(item) => {
                obj.insert("album".to_string(), serde_json::to_value(item)?);
            }
            ClientResponse::Error { code, message } => {
                eprintln!("Error [{code}]: {message}");
                return Ok(());
            }
            _ => {}
        }
        match tracks_resp {
            ClientResponse::CollectionItems(items) => {
                obj.insert("tracks".to_string(), serde_json::to_value(items)?);
            }
            ClientResponse::Error { code, message } => {
                eprintln!("Error fetching tracks [{code}]: {message}");
            }
            _ => {}
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(obj))?
        );
        return Ok(());
    }

    match item_resp {
        ClientResponse::CatalogItem(CatalogItemWire::Album(a)) => {
            println!("Album:       {}", a.id);
            println!("Title:       {}", a.title);
            if !a.artists.is_empty() {
                let artists = a
                    .artists
                    .iter()
                    .map(|art| art.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("Artists:     {artists}");
            }
            if let Some(ref rel) = a.release_date {
                println!("Released:    {rel}");
            }
            if let Some(count) = a.track_count {
                println!("Track Count: {count}");
            }
            if let Some(ref art) = a.artwork {
                println!("Artwork:     {}", art.url);
            }
        }
        ClientResponse::Error { code, message } => {
            eprintln!("Error [{code}]: {message}");
            return Ok(());
        }
        other => {
            println!("Unexpected album response: {other:?}");
            return Ok(());
        }
    }

    match tracks_resp {
        ClientResponse::CollectionItems(tracks) => {
            println!();
            println!("Tracks ({}):", tracks.items.len());
            for (idx, t) in tracks.items.iter().enumerate() {
                let num = t
                    .track_number
                    .map(|n| format!("{n:2}. "))
                    .unwrap_or_else(|| format!("{:2}. ", idx + 1));
                let dur = format_duration_ms(t.duration_ms);
                println!(
                    "  {num}[{}] {} - {} ({dur})",
                    t.id,
                    t.title,
                    t.artist_display()
                );
            }
            if let Some(ref cur) = tracks.next_cursor {
                println!("\nNext cursor: {cur}");
            }
        }
        ClientResponse::Error { code, message } => {
            eprintln!("Error fetching album tracks [{code}]: {message}");
        }
        _ => {}
    }

    Ok(())
}

fn display_artist(item: &CatalogItemWire, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(item)?);
        return Ok(());
    }

    match item {
        CatalogItemWire::Artist(a) => {
            println!("Artist:   {}", a.id);
            println!("Name:     {}", a.name);
            if let Some(ref art) = a.artwork {
                println!("Artwork:  {}", art.url);
            }
        }
        other => {
            println!("Expected artist item, received: {other:?}");
        }
    }
    Ok(())
}

fn display_playlist(
    item_resp: &ClientResponse,
    tracks_resp: &ClientResponse,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let mut obj = serde_json::Map::new();
        match item_resp {
            ClientResponse::CatalogItem(item) => {
                obj.insert("playlist".to_string(), serde_json::to_value(item)?);
            }
            ClientResponse::Error { code, message } => {
                eprintln!("Error [{code}]: {message}");
                return Ok(());
            }
            _ => {}
        }
        match tracks_resp {
            ClientResponse::CollectionItems(items) => {
                obj.insert("tracks".to_string(), serde_json::to_value(items)?);
            }
            ClientResponse::Error { code, message } => {
                eprintln!("Error fetching playlist tracks [{code}]: {message}");
            }
            _ => {}
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(obj))?
        );
        return Ok(());
    }

    match item_resp {
        ClientResponse::CatalogItem(CatalogItemWire::Playlist(p)) => {
            println!("Playlist:    {}", p.id);
            println!("Title:       {}", p.title);
            if let Some(ref curator) = p.curator {
                println!("Curator:     {curator}");
            }
            if let Some(ref desc) = p.description {
                println!("Description: {desc}");
            }
            if let Some(count) = p.track_count {
                println!("Track Count: {count}");
            }
            if let Some(ref art) = p.artwork {
                println!("Artwork:     {}", art.url);
            }
        }
        ClientResponse::Error { code, message } => {
            eprintln!("Error [{code}]: {message}");
            return Ok(());
        }
        other => {
            println!("Unexpected playlist response: {other:?}");
            return Ok(());
        }
    }

    match tracks_resp {
        ClientResponse::CollectionItems(tracks) => {
            println!();
            println!("Tracks ({}):", tracks.items.len());
            for (idx, t) in tracks.items.iter().enumerate() {
                let num = format!("{:2}. ", idx + 1);
                let dur = format_duration_ms(t.duration_ms);
                println!(
                    "  {num}[{}] {} - {} ({dur})",
                    t.id,
                    t.title,
                    t.artist_display()
                );
            }
            if let Some(ref cur) = tracks.next_cursor {
                println!("\nNext cursor: {cur}");
            }
        }
        ClientResponse::Error { code, message } => {
            eprintln!("Error fetching playlist tracks [{code}]: {message}");
        }
        _ => {}
    }

    Ok(())
}

fn display_library(resp: &ClientResponse, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match resp {
        ClientResponse::LibraryPage(page) => {
            if json {
                println!("{}", serde_json::to_string_pretty(page)?);
                return Ok(());
            }

            match page {
                LibraryPageWire::Tracks(page) => {
                    println!("Library Tracks ({} items):", page.items.len());
                    if page.items.is_empty() {
                        println!("  (No tracks found)");
                    } else {
                        for (idx, t) in page.items.iter().enumerate() {
                            let alb = t.album_title().unwrap_or("-");
                            let dur = format_duration_ms(t.duration_ms);
                            println!(
                                "  {:3}. [{}] {} - {} ({}) [{}]",
                                idx + 1,
                                t.id,
                                t.title,
                                t.artist_display(),
                                alb,
                                dur
                            );
                        }
                    }
                    if let Some(ref cur) = page.next_cursor {
                        println!("\nNext cursor: {cur}");
                    }
                }
                LibraryPageWire::Albums(page) => {
                    println!("Library Albums ({} items):", page.items.len());
                    if page.items.is_empty() {
                        println!("  (No albums found)");
                    } else {
                        for (idx, a) in page.items.iter().enumerate() {
                            let artists = a
                                .artists
                                .iter()
                                .map(|art| art.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let count = a
                                .track_count
                                .map(|c| format!("{c} tracks"))
                                .unwrap_or_else(|| "-".to_string());
                            let year = a.release_date.as_deref().unwrap_or("-");
                            println!(
                                "  {:3}. [{}] {} - {} ({year}) [{count}]",
                                idx + 1,
                                a.id,
                                a.title,
                                artists
                            );
                        }
                    }
                    if let Some(ref cur) = page.next_cursor {
                        println!("\nNext cursor: {cur}");
                    }
                }
                LibraryPageWire::Playlists(page) => {
                    println!("Library Playlists ({} items):", page.items.len());
                    if page.items.is_empty() {
                        println!("  (No playlists found)");
                    } else {
                        for (idx, p) in page.items.iter().enumerate() {
                            let curator = p.curator.as_deref().unwrap_or("-");
                            let count = p
                                .track_count
                                .map(|c| format!("{c} tracks"))
                                .unwrap_or_else(|| "-".to_string());
                            println!(
                                "  {:3}. [{}] {} [by {curator}] [{count}]",
                                idx + 1,
                                p.id,
                                p.title
                            );
                        }
                    }
                    if let Some(ref cur) = page.next_cursor {
                        println!("\nNext cursor: {cur}");
                    }
                }
            }
        }
        ClientResponse::Error { code, message } => {
            eprintln!("Error [{code}]: {message}");
        }
        other => print_response(other),
    }

    Ok(())
}

async fn resolve_seek_target(
    client: &mut MalusClient,
    target: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    let t = target.trim();
    if t.starts_with('+') || t.starts_with('-') {
        let is_positive = t.starts_with('+');
        let raw = t[1..].trim();
        let delta_sec: f64 = raw
            .parse()
            .map_err(|_| format!("Invalid seek offset '{t}'"))?;
        let delta_ms = (delta_sec * 1000.0) as u64;

        let status_resp = client.send(&ClientRequest::GetStatus).await?;
        let (current_ms, duration_ms) = match status_resp {
            ClientResponse::Status(s) => (s.position_ms, s.duration_ms),
            _ => (0, u64::MAX),
        };

        let target_ms = if is_positive {
            if duration_ms > 0 {
                (current_ms + delta_ms).min(duration_ms)
            } else {
                current_ms + delta_ms
            }
        } else {
            current_ms.saturating_sub(delta_ms)
        };
        Ok(target_ms)
    } else if t.contains(':') {
        let parts: Vec<&str> = t.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid timestamp '{t}', expected mm:ss").into());
        }
        let mins: u64 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid minutes in '{t}'"))?;
        let secs: f64 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid seconds in '{t}'"))?;
        let total_ms = (mins * 60 * 1000) + (secs * 1000.0) as u64;
        Ok(total_ms)
    } else {
        let secs: f64 = t
            .parse()
            .map_err(|_| format!("Invalid seek target '{t}'"))?;
        Ok((secs * 1000.0) as u64)
    }
}

fn print_response(resp: &ClientResponse) {
    match resp {
        ClientResponse::Ok => println!("ok"),
        ClientResponse::Pong => println!("pong"),
        ClientResponse::Error { code, message } => eprintln!("Error [{code}]: {message}"),
        ClientResponse::Status(s) => println!("{s:?}"),
        ClientResponse::Queue(q) => println!("{q:?}"),
        ClientResponse::SearchResults(results) => println!("{results:?}"),
        ClientResponse::CatalogItem(item) => println!("{item:?}"),
        ClientResponse::CollectionItems(items) => println!("{items:?}"),
        ClientResponse::LibraryPage(page) => println!("{page:?}"),
        ClientResponse::ActionResult(res) => println!("{res:?}"),
        ClientResponse::AuthStatus(s) => {
            println!("Auth status: {}", s.state);
            if let Some(msg) = &s.message {
                println!("  Message: {msg}");
            }
        }
        ClientResponse::Navigation(nav) => println!("{nav:?}"),
        ClientResponse::Page(page) => println!("{page:?}"),
        ClientResponse::PageContinued(cont) => println!("{cont:?}"),
    }
}

fn run_doctor() {
    println!("Malus diagnostics\n");

    let wpe_cand = malus_wpe::discover_wpe(None);
    if wpe_cand.is_some() {
        println!("{:<18} OK", "WPE runtime");
    } else {
        println!("{:<18} MISSING", "WPE runtime");
    }

    let bwrap_paths = ["/usr/bin/bwrap", "/usr/sbin/bwrap", "/bin/bwrap"];
    let bwrap_ok = bwrap_paths.iter().any(|p| Path::new(p).is_file());
    if bwrap_ok {
        println!("{:<18} OK", "WebKit sandbox");
    } else {
        println!("{:<18} MISSING", "WebKit sandbox");
    }

    let ocdm_ok = wpe_cand
        .as_ref()
        .and_then(|c| c.ocdm_path.as_ref())
        .map(|p| p.is_file())
        .unwrap_or(false);
    if ocdm_ok {
        println!("{:<18} OK", "OpenCDM");
    } else {
        println!("{:<18} MISSING", "OpenCDM");
    }

    let persisted_status = malus_wpe::check_persisted_widevine_status();
    let widevine_res = malus_wpe::discover_widevine();
    let mut show_setup_hint = false;

    match widevine_res {
        Ok(inst) => {
            if let malus_wpe::PersistedWidevineStatus::Invalid(ref bad_path) = persisted_status {
                println!(
                    "{:<18} OK  {} (persisted config '{}' is invalid)",
                    "Widevine CDM",
                    inst.library_path.display(),
                    bad_path.display()
                );
            } else if inst.source == malus_wpe::WidevineSource::ManagedInstall {
                if let Some(meta) = malus_wpe::read_managed_metadata(&inst.directory) {
                    println!(
                        "{:<18} OK  {} (source: Managed install, version: {})",
                        "Widevine CDM",
                        inst.library_path.display(),
                        meta.version
                    );
                } else {
                    println!(
                        "{:<18} OK  {} (source: Managed install)",
                        "Widevine CDM",
                        inst.library_path.display()
                    );
                }
            } else {
                println!("{:<18} OK  {}", "Widevine CDM", inst.library_path.display());
            }
        }
        Err(malus_wpe::WidevineError::InvalidPath(msg)) => {
            println!("{:<18} INVALID CONFIG ({msg})", "Widevine CDM");
            show_setup_hint = true;
        }
        Err(_) => {
            if let malus_wpe::PersistedWidevineStatus::Invalid(ref bad_path) = persisted_status {
                println!(
                    "{:<18} INVALID CONFIG ({})",
                    "Widevine CDM",
                    bad_path.display()
                );
            } else {
                println!("{:<18} MISSING", "Widevine CDM");
            }
            show_setup_hint = true;
        }
    }

    let malusd_ok = check_daemon_binary();
    if malusd_ok {
        println!("{:<18} OK", "Malus daemon");
    } else {
        println!("{:<18} MISSING", "Malus daemon");
    }

    if show_setup_hint {
        println!("\nRun: malusctl setup-widevine --install");
    }
}

fn check_daemon_binary() -> bool {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
        && parent.join("malusd").is_file()
    {
        return true;
    }
    if Path::new("target/debug/malusd").is_file() || Path::new("target/release/malusd").is_file() {
        return true;
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            if Path::new(dir).join("malusd").is_file() {
                return true;
            }
        }
    }
    false
}

async fn run_setup_widevine(
    path: Option<PathBuf>,
    install: bool,
    accept_google_terms: bool,
    force: bool,
    reset: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Malus Widevine Setup\n");

    if path.is_some() && install {
        eprintln!("Error: --path and --install cannot be used together.");
        std::process::exit(1);
    }
    if reset && install {
        eprintln!("Error: --reset and --install cannot be used together.");
        std::process::exit(1);
    }
    if reset && path.is_some() {
        eprintln!("Error: --reset and --path cannot be used together.");
        std::process::exit(1);
    }
    if force && !install {
        eprintln!("Error: --force requires --install.");
        std::process::exit(1);
    }
    if accept_google_terms && !install {
        eprintln!("Error: --accept-google-terms requires --install.");
        std::process::exit(1);
    }

    if reset {
        match malus_wpe::reset_widevine_config() {
            Ok(res) => {
                if let Some(ref dir) = res.managed_files_removed {
                    println!(
                        "Removed Malus-managed Widevine directory: {}",
                        dir.display()
                    );
                }
                if let Some(ref ext) = res.preserved_external_path {
                    println!(
                        "Removed persisted configuration. External library was preserved: {}",
                        ext.display()
                    );
                }
                if res.config_removed {
                    println!("Persisted configuration removed.");
                } else {
                    println!("No persisted configuration was present.");
                }
                println!("\nWidevine configuration reset complete.");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error during reset: {e}");
                std::process::exit(1);
            }
        }
    }

    if install {
        use std::io::IsTerminal;
        if !accept_google_terms {
            if !std::io::stdin().is_terminal() {
                eprintln!(
                    "Error: --accept-google-terms is required for non-interactive installation."
                );
                std::process::exit(1);
            }

            println!("Widevine is proprietary software provided by Google.\n");
            println!("Malus can download Google's official Linux Chrome package and extract");
            println!("only the Widevine CDM for local use. Chrome itself will not be installed.\n");
            println!("Downloading Google's package is subject to Google's terms:");
            println!("https://www.google.com/chrome/terms/\n");
            print!("Continue? [y/N]: ");
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let trimmed = input.trim();
            if trimmed != "y" && trimmed != "Y" {
                println!("Installation cancelled.");
                return Ok(());
            }
        }

        println!("Downloading and extracting Widevine CDM from Google's official package...");
        match malus_wpe::install_managed_widevine(None, force).await {
            Ok(inst) => {
                println!("\nWidevine CDM successfully configured:");
                println!("  Source:       {}", inst.source);
                println!("  Library path: {}", inst.library_path.display());
                println!("  Sandbox root: {}", inst.directory.display());
                if let Some(meta) = malus_wpe::read_managed_metadata(&inst.directory) {
                    println!("  Version:      {}", meta.version);
                    println!("  Installed at: {}", meta.installed_at);
                }
                if let Some(cfg) = malus_wpe::widevine::get_user_config_path() {
                    println!("  Persisted in: {}", cfg.display());
                }
                println!("\nWidevine is ready for Apple Music playback.");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error installing Widevine: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(custom_path) = path {
        println!(
            "Validating provided Widevine path: {}",
            custom_path.display()
        );
        match malus_wpe::persist_widevine_path(&custom_path) {
            Ok(inst) => {
                println!("Widevine CDM validated and configured:");
                println!("  Library path: {}", inst.library_path.display());
                println!("  Sandbox root: {}", inst.directory.display());
                if let Some(cfg) = malus_wpe::widevine::get_user_config_path() {
                    println!("  Persisted in: {}", cfg.display());
                }
                println!("\nWidevine is ready for Apple Music playback.");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error configuring Widevine: {e}");
                std::process::exit(1);
            }
        }
    }

    match malus_wpe::discover_widevine() {
        Ok(inst) => {
            println!("Found existing Widevine CDM installation:");
            println!("  Source:       {}", inst.source);
            println!("  Library path: {}", inst.library_path.display());
            println!("  Sandbox root: {}", inst.directory.display());
            println!("  Validation:   OK (regular ELF shared library)");
            println!("\nWidevine is available and ready for playback.");
            Ok(())
        }
        Err(e) => {
            println!("No working Widevine CDM installation was detected ({e}).");
            println!("\nApple Music playback requires libwidevinecdm.so.");
            println!("You can install Widevine automatically from Google's official package with:");
            println!("\n  malusctl setup-widevine --install");
            println!("\nOr configure an existing Widevine CDM installation with:");
            println!("\n  malusctl setup-widevine --path /path/to/libwidevinecdm.so");
            println!("\nKnown sources for libwidevinecdm.so on Linux:");
            println!("  - Installed browser packages (Google Chrome, Chromium, Brave, Vivaldi)");
            println!("  - Distribution Widevine packages (e.g. chromium-widevine, widevine)");
            println!("  - Custom or system library directories");

            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() {
                println!("\nEnter path to libwidevinecdm.so (or press Enter to cancel): ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    match malus_wpe::persist_widevine_path(Path::new(trimmed)) {
                        Ok(inst) => {
                            println!("\nWidevine CDM validated and configured:");
                            println!("  Library path: {}", inst.library_path.display());
                            println!("  Sandbox root: {}", inst.directory.display());
                            if let Some(cfg) = malus_wpe::widevine::get_user_config_path() {
                                println!("  Persisted in: {}", cfg.display());
                            }
                            println!("\nWidevine is ready for Apple Music playback.");
                            return Ok(());
                        }
                        Err(err) => {
                            eprintln!("Error configuring Widevine: {err}");
                            std::process::exit(1);
                        }
                    }
                }
            }
            std::process::exit(1);
        }
    }
}
