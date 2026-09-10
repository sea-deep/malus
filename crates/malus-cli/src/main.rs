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
enum Commands {
    /// Resume or start playback
    Play,

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

    /// Seek to a specific position in milliseconds
    Seek {
        #[arg(help = "Position in milliseconds")]
        position_ms: u64,
    },

    /// Set volume (0 - 100)
    Volume {
        #[arg(help = "Volume level from 0 to 100")]
        volume: u8,
    },

    /// Display current player status
    Status,

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
        Commands::Play => {
            let resp = client.send(&ClientRequest::Play).await?;
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
        Commands::Seek { position_ms } => {
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
                    println!(
                        "  - {} ({}): {} {:?}",
                        p.id, p.name, p.state, p.capabilities
                    );
                }
            }
        }
        ClientResponse::ActionResult(val) => println!("{val}"),
    }
}
