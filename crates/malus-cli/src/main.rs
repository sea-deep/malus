//! Malus Command Line Interface.

use clap::{Parser, Subcommand};
use malus_cli::{Client, default_socket_path};
use malus_protocol::{
    MediaIdWire, TrackWire,
    client::{ClientRequest, ClientResponse},
    wire::ActionRequestV0,
};
use std::path::PathBuf;

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
enum Commands {
    /// Resume playback, or play a specified media ID (e.g. apple:track:1440857781)
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let socket_path = cli.socket.unwrap_or_else(default_socket_path);

    let mut client = Client::connect(&socket_path).await.map_err(|e| {
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
                        Some(t) => format!("{} - {}", t.title, t.artist),
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
                                track.artist
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
        Commands::Search { query } => {
            let resp = client.send(&ClientRequest::Search { query }).await?;
            match resp {
                ClientResponse::SearchResults { tracks } => {
                    if tracks.is_empty() {
                        println!("No tracks found.");
                    } else {
                        for (idx, t) in tracks.iter().enumerate() {
                            let album = t.album.as_deref().unwrap_or("-");
                            println!(
                                "{:2}. [{}] {} - {} ({})",
                                idx + 1,
                                t.id,
                                t.title,
                                t.artist,
                                album
                            );
                        }
                    }
                }
                other => print_response(&other),
            }
        }
        Commands::Enqueue { id, title, artist } => {
            let track = TrackWire {
                id,
                title: title.unwrap_or_else(|| "Unknown Track".into()),
                artist: artist.unwrap_or_else(|| "Unknown Artist".into()),
                album: None,
                duration_ms: None,
                uri: None,
            };
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
                                    Some(t) => format!("{} - {}", t.title, t.artist),
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
    }

    Ok(())
}

async fn resolve_seek_target(
    client: &mut Client,
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
        ClientResponse::SearchResults { tracks } => println!("{tracks:?}"),
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
    }
}
