//! Malus Daemon playback engine and provider coordinator.
//!
//! Communicates with frontends via `client::` RPC and with providers
//! exclusively through `provider::` wire protocol process channels.
//! Strictly does NOT depend on `malus-provider-sdk`.

use crate::provider_process::ProviderProcess;
use malus_protocol::{
    client::{ClientEvent, ClientRequest, ClientResponse},
    wire::{PlayerStatusWire, ProviderInfoWire, QueueWire},
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn};

pub struct Engine {
    providers: RwLock<HashMap<String, Arc<ProviderProcess>>>,
    active_provider: RwLock<Option<String>>,
    event_tx: broadcast::Sender<ClientEvent>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(128);
        Self {
            providers: RwLock::new(HashMap::new()),
            active_provider: RwLock::new(None),
            event_tx,
        }
    }

    /// Register a running provider process.
    pub async fn register_provider(&self, provider: Arc<ProviderProcess>) {
        info!(
            "Registering provider process: {} ({})",
            provider.name(),
            provider.id()
        );
        let id = provider.id().to_string();
        let mut map = self.providers.write().await;
        map.insert(id.clone(), provider);

        let mut active = self.active_provider.write().await;
        if active.is_none() {
            *active = Some(id);
        }
    }

    /// Set the currently active audio provider.
    pub async fn set_active_provider(&self, id: &str) -> bool {
        let map = self.providers.read().await;
        if map.contains_key(id) {
            let mut active = self.active_provider.write().await;
            *active = Some(id.to_string());
            true
        } else {
            false
        }
    }

    pub async fn get_active_provider(&self) -> Option<Arc<ProviderProcess>> {
        let active = self.active_provider.read().await;
        if let Some(ref id) = *active {
            let map = self.providers.read().await;
            map.get(id).cloned()
        } else {
            None
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.event_tx.subscribe()
    }

    pub fn emit(&self, event: ClientEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Process a client protocol request.
    pub async fn handle_request(&self, req: ClientRequest) -> ClientResponse {
        match req {
            ClientRequest::Ping => ClientResponse::Pong,

            ClientRequest::GetCapabilities { provider } => {
                let map = self.providers.read().await;
                if let Some(p) = map.get(&provider) {
                    ClientResponse::Capabilities {
                        provider,
                        capabilities: p.capabilities().await,
                    }
                } else {
                    ClientResponse::err(
                        "PROVIDER_NOT_FOUND",
                        format!("Provider '{provider}' not found"),
                    )
                }
            }

            ClientRequest::ListProviders => {
                let map = self.providers.read().await;
                let mut list = Vec::new();
                for (id, p) in map.iter() {
                    list.push(ProviderInfoWire {
                        id: id.clone(),
                        name: p.name().to_string(),
                        state: p.state().await.to_string(),
                        capabilities: p.capabilities().await,
                    });
                }
                ClientResponse::Providers(list)
            }

            ClientRequest::Search { query } => {
                let map = self.providers.read().await;
                let mut all_tracks = Vec::new();
                for (id, p) in map.iter() {
                    match p.search(&query).await {
                        Ok(tracks) => all_tracks.extend(tracks),
                        Err(e) => {
                            warn!("Search on provider '{id}' failed: {e}");
                        }
                    }
                }
                ClientResponse::SearchResults { tracks: all_tracks }
            }

            ClientRequest::Play => {
                if let Some(p) = self.get_active_provider().await {
                    match p.resume().await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("PLAY_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::PlayTrack { media_id } => {
                if let Some(p) = self.get_active_provider().await {
                    match p.play_track(&media_id).await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("PLAY_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Pause => {
                if let Some(p) = self.get_active_provider().await {
                    match p.pause().await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("PAUSE_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::TogglePlay => {
                if let Some(p) = self.get_active_provider().await {
                    match p.toggle_play().await {
                        Ok(()) => ClientResponse::Ok,
                        Err(e) => ClientResponse::err("TOGGLE_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Stop => {
                if let Some(p) = self.get_active_provider().await {
                    match p.stop().await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("STOP_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Next => {
                if let Some(p) = self.get_active_provider().await {
                    match p.next().await {
                        Ok(()) => ClientResponse::Ok,
                        Err(e) => ClientResponse::err("NEXT_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Previous => {
                if let Some(p) = self.get_active_provider().await {
                    match p.previous().await {
                        Ok(()) => ClientResponse::Ok,
                        Err(e) => ClientResponse::err("PREVIOUS_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Seek { position_ms } => {
                if let Some(p) = self.get_active_provider().await {
                    match p.seek(position_ms).await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("SEEK_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::SetVolume { volume } => {
                if let Some(p) = self.get_active_provider().await {
                    match p.set_volume(volume).await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("VOLUME_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::SetShuffle { .. } => ClientResponse::Ok,
            ClientRequest::SetRepeat { .. } => ClientResponse::Ok,
            ClientRequest::ClearQueue => ClientResponse::Ok,

            ClientRequest::GetStatus => {
                if let Some(p) = self.get_active_provider().await {
                    match p.get_status().await {
                        Ok(status) => ClientResponse::Status(status),
                        Err(e) => ClientResponse::err("STATUS_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::Status(PlayerStatusWire {
                        state: malus_protocol::PlaybackStateWire::Stopped,
                        current_track: None,
                        position_ms: 0,
                        duration_ms: 0,
                        volume: 100,
                        muted: false,
                        shuffle: false,
                        repeat: malus_protocol::RepeatModeWire::Off,
                    })
                }
            }

            ClientRequest::GetQueue => {
                if let Some(p) = self.get_active_provider().await {
                    match p.get_queue().await {
                        Ok(queue) => ClientResponse::Queue(queue),
                        Err(e) => ClientResponse::err("QUEUE_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::Queue(QueueWire {
                        items: Vec::new(),
                        current_index: None,
                    })
                }
            }

            ClientRequest::Enqueue { track } => {
                if let Some(p) = self.get_active_provider().await {
                    match p.enqueue(track).await {
                        Ok(()) => {
                            if let Ok(queue) = p.get_queue().await {
                                self.emit(ClientEvent::QueueChanged(queue));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("ENQUEUE_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Action(action_req) => {
                let map = self.providers.read().await;
                if let Some(p) = map.get(&action_req.provider) {
                    match p
                        .custom_action(&action_req.action, action_req.target, action_req.params)
                        .await
                    {
                        Ok(result) => ClientResponse::ActionResult(result),
                        Err(e) => ClientResponse::err("ACTION_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err(
                        "PROVIDER_NOT_FOUND",
                        format!("Provider '{}' not found", action_req.provider),
                    )
                }
            }

            ClientRequest::SubscribeEvents => ClientResponse::Ok,
        }
    }

    /// Gracefully shut down all registered provider processes.
    pub async fn shutdown(&self) {
        let map = self.providers.read().await;
        for (id, p) in map.iter() {
            info!("Shutting down provider '{id}'...");
            let _ = p.shutdown().await;
        }
    }
}
