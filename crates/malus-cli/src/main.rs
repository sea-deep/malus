//! Malus Command Line Interface.

use clap::{Parser, Subcommand};
use malus_cli::{MalusClient, default_socket_path};
use malus_protocol::{
    client::{ClientRequest, ClientResponse},
    wire::{
        ActionRequestV0, CatalogItemWire, LibraryKindWire, LibraryPageWire, MediaIdWire,
        SearchKindWire, SearchResultsWire, TrackWire,
    },
};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "malus", about = "Malus audio player command-line interface")]
struct Cli {
    /// Path to malus daemon UNIX domain socket
    #[arg(short, long, global = true)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// List all discovered and active providers
    List,

    /// Initiate interactive authentication for a provider
    Login {
        #[arg(help = "Provider ID (e.g. apple)")]
        provider: String,
    },

    /// Log out from a provider and clear session data
    Logout {
        #[arg(help = "Provider ID (e.g. apple)")]
        provider: String,
    },

    /// Inspect provider status, capabilities, and auth state
    Info {
        #[arg(help = "Provider ID (e.g. apple)")]
        provider: String,
    },
}

#[derive(Subcommand)]
enum LibraryCommands {
    /// Browse saved library tracks
    #[command(alias = "track")]
    Tracks {
        /// Provider ID to query (defaults to active provider)
        #[arg(short = 'p', long = "provider")]
        provider: Option<String>,

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
        /// Provider ID to query (defaults to active provider)
        #[arg(short = 'p', long = "provider")]
        provider: Option<String>,

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
        /// Provider ID to query (defaults to active provider)
        #[arg(short = 'p', long = "provider")]
        provider: Option<String>,

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

#[derive(Subcommand)]
enum Commands {
    /// Resume playback, or play a specified media ID (e.g. apple:track:1440857781)
    #[command(alias = "resume")]
    Play {
        #[arg(help = "Optional media ID to play (e.g. apple:track:1440857781)")]
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

    /// Seek to a specific position (seconds or mm:ss) or offset (+15, -10)
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

    /// Display the current playback queue
    Queue,

    /// Clear all tracks from the queue
    Clear,

    /// Search audio catalog
    Search {
        #[arg(help = "Search query string")]
        query: String,

        /// Filter by media type (track, album, artist, playlist)
        #[arg(short = 't', long = "type")]
        r#type: Option<String>,

        /// Provider ID to query (defaults to active provider)
        #[arg(short = 'p', long = "provider")]
        provider: Option<String>,

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
        #[arg(help = "Track media ID (e.g. apple:track:1440857781)")]
        media_id: String,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect catalog album details and tracklist
    Album {
        #[arg(help = "Album media ID (e.g. apple:album:1440857780)")]
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
        #[arg(help = "Artist media ID (e.g. apple:artist:5468295)")]
        media_id: String,

        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect catalog playlist details and tracklist
    Playlist {
        #[arg(help = "Playlist media ID (e.g. apple:playlist:pl.xyz)")]
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

    /// Enqueue a track
    Enqueue {
        #[arg(help = "Track ID (e.g. mock:track:1)")]
        id: String,
        #[arg(short, long, help = "Track title")]
        title: Option<String>,
        #[arg(short, long, help = "Track artist")]
        artist: Option<String>,
    },

    /// List active providers registered with daemon
    Providers,

    /// Manage audio and metadata providers
    Provider {
        #[command(subcommand)]
        action: ProviderCommands,
    },

    /// Execute a provider custom action
    Action {
        #[arg(help = "Target provider name (e.g. mock)")]
        provider: String,
        #[arg(help = "Action name (e.g. mock.repost)")]
        action: String,
        #[arg(short, long, help = "Normalized target media ID (e.g. mock:track:3)")]
        target: Option<String>,
        #[arg(short, long, help = "JSON parameters string")]
        params: Option<String>,
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
            "Ensure 'malus-daemon' is running and socket exists at {}",
            socket_path.display()
        );
        e
    })?;

    match cli.command {
        Commands::Play { media_id } => {
            let req = match media_id {
                Some(id) => ClientRequest::PlayTrack { media_id: id },
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
        Commands::Queue => {
            let resp = client.send(&ClientRequest::GetQueue).await?;
            match resp {
                ClientResponse::Queue(q) => {
                    if q.items.is_empty() {
                        println!("Queue is empty.");
                    } else {
                        for (idx, track) in q.items.iter().enumerate() {
                            let marker = if q.current_index == Some(idx) {
                                ">"
                            } else {
                                " "
                            };
                            println!(
                                "{} {:2}. [{}] {} - {}",
                                marker,
                                idx + 1,
                                track.id,
                                track.title,
                                track.artist_display()
                            );
                        }
                    }
                }
                other => print_response(&other),
            }
        }
        Commands::Clear => {
            let resp = client.send(&ClientRequest::ClearQueue).await?;
            print_response(&resp);
        }
        Commands::Search {
            query,
            r#type,
            provider,
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
                    provider,
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
            let resp = client
                .send(&ClientRequest::GetCatalogItem { media_id })
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
            let item_resp = client
                .send(&ClientRequest::GetCatalogItem {
                    media_id: media_id.clone(),
                })
                .await?;
            let tracks_resp = client
                .send(&ClientRequest::GetCollectionItems {
                    media_id,
                    limit,
                    cursor,
                })
                .await?;
            display_album(&item_resp, &tracks_resp, json)?;
        }
        Commands::Artist { media_id, json } => {
            let resp = client
                .send(&ClientRequest::GetCatalogItem { media_id })
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
            let item_resp = client
                .send(&ClientRequest::GetCatalogItem {
                    media_id: media_id.clone(),
                })
                .await?;
            let tracks_resp = client
                .send(&ClientRequest::GetCollectionItems {
                    media_id,
                    limit,
                    cursor,
                })
                .await?;
            display_playlist(&item_resp, &tracks_resp, json)?;
        }
        Commands::Library { action } => match action {
            LibraryCommands::Tracks {
                provider,
                limit,
                cursor,
                json,
            } => {
                let resp = client
                    .send(&ClientRequest::GetLibrary {
                        kind: LibraryKindWire::Tracks,
                        provider,
                        limit,
                        cursor,
                    })
                    .await?;
                display_library(&resp, json)?;
            }
            LibraryCommands::Albums {
                provider,
                limit,
                cursor,
                json,
            } => {
                let resp = client
                    .send(&ClientRequest::GetLibrary {
                        kind: LibraryKindWire::Albums,
                        provider,
                        limit,
                        cursor,
                    })
                    .await?;
                display_library(&resp, json)?;
            }
            LibraryCommands::Playlists {
                provider,
                limit,
                cursor,
                json,
            } => {
                let resp = client
                    .send(&ClientRequest::GetLibrary {
                        kind: LibraryKindWire::Playlists,
                        provider,
                        limit,
                        cursor,
                    })
                    .await?;
                display_library(&resp, json)?;
            }
        },
        Commands::Enqueue { id, title, artist } => {
            let track = TrackWire::new(
                id,
                title.unwrap_or_else(|| "Unknown Track".into()),
                artist.unwrap_or_else(|| "Unknown Artist".into()),
            );
            let resp = client.send(&ClientRequest::Enqueue { track }).await?;
            print_response(&resp);
        }
        Commands::Providers => {
            let resp = client.send(&ClientRequest::ListProviders).await?;
            print_response(&resp);
        }
        Commands::Provider { action } => match action {
            ProviderCommands::List => {
                let resp = client.send(&ClientRequest::ListProviders).await?;
                match resp {
                    ClientResponse::Providers(providers) => {
                        if providers.is_empty() {
                            println!("No providers found.");
                        } else {
                            println!("{:<12} {:<12} {:<15} CAPABILITIES", "ID", "STATE", "AUTH");
                            println!("{:-<12} {:-<12} {:-<15} {:-<20}", "", "", "", "");
                            for p in providers {
                                let auth_str = p
                                    .auth_state
                                    .as_ref()
                                    .map(|a| a.to_string())
                                    .unwrap_or_else(|| "-".to_string());
                                println!(
                                    "{:<12} {:<12} {:<15} {:?}",
                                    p.id, p.state, auth_str, p.capabilities
                                );
                            }
                        }
                    }
                    other => print_response(&other),
                }
            }
            ProviderCommands::Login { provider } => {
                println!("Initiating login for provider '{provider}'...");
                let resp = client
                    .send(&ClientRequest::AuthBegin {
                        provider: provider.clone(),
                    })
                    .await?;
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
            ProviderCommands::Logout { provider } => {
                println!("Logging out provider '{provider}'...");
                let resp = client
                    .send(&ClientRequest::AuthLogout {
                        provider: provider.clone(),
                    })
                    .await?;
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
            ProviderCommands::Info { provider } => {
                let auth_resp = client
                    .send(&ClientRequest::GetAuthStatus {
                        provider: provider.clone(),
                    })
                    .await?;
                let caps_resp = client
                    .send(&ClientRequest::GetCapabilities {
                        provider: provider.clone(),
                    })
                    .await?;

                println!("Provider: {provider}");
                match auth_resp {
                    ClientResponse::AuthStatus(s) => {
                        println!("  Auth State: {}", s.state);
                        if let Some(msg) = s.message {
                            println!("  Details:    {msg}");
                        }
                    }
                    ClientResponse::Error { code, message } => {
                        println!("  Auth Error [{code}]: {message}");
                    }
                    other => print_response(&other),
                }
                match caps_resp {
                    ClientResponse::Capabilities { capabilities, .. } => {
                        println!("  Capabilities: {:?}", capabilities);
                    }
                    ClientResponse::Error { code, message } => {
                        println!("  Caps Error [{code}]: {message}");
                    }
                    other => print_response(&other),
                }
            }
        },
        Commands::Action {
            provider,
            action,
            target,
            params,
        } => {
            let json_params = if let Some(p) = params {
                serde_json::from_str(&p)?
            } else {
                serde_json::json!({})
            };
            let target_wire = target.map(|t| MediaIdWire::parse(&t)).transpose()?;
            let action_req = ActionRequestV0 {
                provider,
                action,
                target: target_wire,
                params: json_params,
            };
            let resp = client.send(&ClientRequest::Action(action_req)).await?;
            print_response(&resp);
        }
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
                            malus_protocol::client::ClientEvent::StatusChanged(s) => {
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
                            malus_protocol::client::ClientEvent::AuthChanged(auth) => {
                                println!("[Auth] provider={}: {}", auth.provider, auth.state);
                            }
                            malus_protocol::client::ClientEvent::QueueChanged(q) => {
                                println!("[Queue] {} tracks", q.items.len());
                            }
                            other => println!("[Event] {other:?}"),
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
        ClientResponse::Capabilities {
            provider,
            capabilities,
        } => println!("Capabilities for {provider}: {capabilities:?}"),
        ClientResponse::Providers(providers) => {
            if providers.is_empty() {
                println!("No providers registered.");
            } else {
                println!("Registered providers:");
                for p in providers {
                    let auth_str = p
                        .auth_state
                        .as_ref()
                        .map(|a| a.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "  - {} ({}): {} [auth: {}] {:?}",
                        p.id, p.name, p.state, auth_str, p.capabilities
                    );
                }
            }
        }
        ClientResponse::ActionResult(val) => println!("{val}"),
        ClientResponse::AuthStatus(s) => {
            println!("Auth status for {}: {}", s.provider, s.state);
            if let Some(msg) = &s.message {
                println!("  Message: {msg}");
            }
        }
        ClientResponse::ProviderSurfaceManifest(manifest) => println!("{manifest:?}"),
        ClientResponse::Surface(surface) => println!("{surface:?}"),
        ClientResponse::SurfaceContinued(cont) => println!("{cont:?}"),
        ClientResponse::SurfaceActionResult(res) => println!("{res:?}"),
    }
}

fn run_doctor() {
    println!("Malus diagnostics\n");

    let wpe_cand = malus_web_runtime::discover_wpe(None);
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

    let persisted_status = malus_web_runtime::check_persisted_widevine_status();
    let widevine_res = malus_web_runtime::discover_widevine();
    let mut show_setup_hint = false;

    match widevine_res {
        Ok(inst) => {
            if let malus_web_runtime::PersistedWidevineStatus::Invalid(ref bad_path) =
                persisted_status
            {
                println!(
                    "{:<18} OK  {} (persisted config '{}' is invalid)",
                    "Widevine CDM",
                    inst.library_path.display(),
                    bad_path.display()
                );
            } else if inst.source == malus_web_runtime::WidevineSource::ManagedInstall {
                if let Some(meta) = malus_web_runtime::read_managed_metadata(&inst.directory) {
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
        Err(malus_web_runtime::WidevineError::InvalidPath(msg)) => {
            println!("{:<18} INVALID CONFIG ({msg})", "Widevine CDM");
            show_setup_hint = true;
        }
        Err(_) => {
            if let malus_web_runtime::PersistedWidevineStatus::Invalid(ref bad_path) =
                persisted_status
            {
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

    let apple_provider_ok = check_apple_provider_binary();
    if apple_provider_ok {
        println!("{:<18} OK", "Apple provider");
    } else {
        println!("{:<18} MISSING", "Apple provider");
    }

    if show_setup_hint {
        println!("\nRun: malus setup-widevine --install");
    }
}

fn check_apple_provider_binary() -> bool {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
        && parent.join("malus-provider-apple").is_file()
    {
        return true;
    }
    if Path::new("target/debug/malus-provider-apple").is_file()
        || Path::new("target/release/malus-provider-apple").is_file()
    {
        return true;
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            if Path::new(dir).join("malus-provider-apple").is_file() {
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

    // Phase 11: Validate CLI arguments
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

    // Phase 12: Handle --reset
    if reset {
        match malus_web_runtime::reset_widevine_config() {
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

    // Phase 10: Handle --install
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
        match malus_web_runtime::install_managed_widevine(None, force).await {
            Ok(inst) => {
                println!("\nWidevine CDM successfully configured:");
                println!("  Source:       {}", inst.source);
                println!("  Library path: {}", inst.library_path.display());
                println!("  Sandbox root: {}", inst.directory.display());
                if let Some(meta) = malus_web_runtime::read_managed_metadata(&inst.directory) {
                    println!("  Version:      {}", meta.version);
                    println!("  Installed at: {}", meta.installed_at);
                }
                if let Some(cfg) = malus_web_runtime::widevine::get_user_config_path() {
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

    // Existing --path handler
    if let Some(custom_path) = path {
        println!(
            "Validating provided Widevine path: {}",
            custom_path.display()
        );
        match malus_web_runtime::persist_widevine_path(&custom_path) {
            Ok(inst) => {
                println!("Widevine CDM validated and configured:");
                println!("  Library path: {}", inst.library_path.display());
                println!("  Sandbox root: {}", inst.directory.display());
                if let Some(cfg) = malus_web_runtime::widevine::get_user_config_path() {
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

    // Default discovery
    match malus_web_runtime::discover_widevine() {
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
            println!("\n  malus setup-widevine --install");
            println!("\nOr configure an existing Widevine CDM installation with:");
            println!("\n  malus setup-widevine --path /path/to/libwidevinecdm.so");
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
                    match malus_web_runtime::persist_widevine_path(Path::new(trimmed)) {
                        Ok(inst) => {
                            println!("\nWidevine CDM validated and configured:");
                            println!("  Library path: {}", inst.library_path.display());
                            println!("  Sandbox root: {}", inst.directory.display());
                            if let Some(cfg) = malus_web_runtime::widevine::get_user_config_path() {
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
