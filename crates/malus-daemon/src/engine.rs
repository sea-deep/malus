//! Malus Daemon playback engine and provider coordinator.
//!
//! Communicates with frontends via `client::` RPC and with providers
//! exclusively through `provider::` wire protocol process channels.
//! Strictly does NOT depend on `malus-provider-sdk`.
//!
//! Invariant: `installed != running`.
//! Inactive discovered providers consume 0 processes until activated, queried,
//! or authenticated.

use crate::{
    discovery::{DiscoveredProvider, discover_providers},
    provider_process::ProviderProcess,
};
use malus_protocol::{
    client::{ClientEvent, ClientRequest, ClientResponse},
    wire::{PlayerStatusWire, ProviderInfoWire, QueueWire},
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct MirroredPlayerState {
    pub status: PlayerStatusWire,
    pub received_at: std::time::Instant,
}

impl MirroredPlayerState {
    pub fn new(status: PlayerStatusWire) -> Self {
        Self {
            status,
            received_at: std::time::Instant::now(),
        }
    }

    pub fn extrapolated_status(&self) -> PlayerStatusWire {
        let mut s = self.status.clone();
        if s.state == malus_protocol::PlaybackStateWire::Playing {
            let elapsed_ms = self.received_at.elapsed().as_millis() as u64;
            let mut pos = s.position_ms + elapsed_ms;
            if s.duration_ms > 0 && pos > s.duration_ms {
                pos = s.duration_ms;
            }
            s.position_ms = pos;
        }
        s
    }
}

pub struct Engine {
    discovered: RwLock<HashMap<String, DiscoveredProvider>>,
    running: RwLock<HashMap<String, Arc<ProviderProcess>>>,
    active_provider: Arc<RwLock<Option<String>>>,
    mirrored_player: Arc<RwLock<Option<MirroredPlayerState>>>,
    event_tx: broadcast::Sender<ClientEvent>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Initialize the engine with automatic host provider discovery.
    pub fn new() -> Self {
        let discovered = discover_providers(None);
        Self::with_discovered(discovered)
    }

    /// Initialize the engine with a pre-configured map of discovered providers.
    pub fn with_discovered(discovered: HashMap<String, DiscoveredProvider>) -> Self {
        let (event_tx, _) = broadcast::channel(128);
        Self {
            discovered: RwLock::new(discovered),
            running: RwLock::new(HashMap::new()),
            active_provider: Arc::new(RwLock::new(None)),
            mirrored_player: Arc::new(RwLock::new(None)),
            event_tx,
        }
    }

    pub async fn update_mirrored_status(&self, status: PlayerStatusWire) {
        let mut guard = self.mirrored_player.write().await;
        *guard = Some(MirroredPlayerState::new(status));
    }

    fn wire_provider_events(&self, p_id: String, proc: &Arc<ProviderProcess>) {
        let event_tx = self.event_tx.clone();
        let mirrored = self.mirrored_player.clone();
        let active = self.active_provider.clone();

        let cb = Arc::new(
            move |event: malus_protocol::provider::ProviderEvent| match event {
                malus_protocol::provider::ProviderEvent::StatusChanged(status) => {
                    let is_active = {
                        let guard = active.try_read();
                        guard.map(|a| a.as_deref() == Some(&p_id)).unwrap_or(true)
                    };
                    if is_active && let Ok(mut m) = mirrored.try_write() {
                        *m = Some(MirroredPlayerState::new(status.clone()));
                    }
                    let _ = event_tx.send(ClientEvent::StatusChanged(status));
                }
                malus_protocol::provider::ProviderEvent::AuthChanged(auth) => {
                    let _ = event_tx.send(ClientEvent::AuthChanged(auth));
                }
                malus_protocol::provider::ProviderEvent::QueueChanged(queue) => {
                    let _ = event_tx.send(ClientEvent::QueueChanged(queue));
                }
                malus_protocol::provider::ProviderEvent::NeedsAuth { message } => {
                    let _ = event_tx.send(ClientEvent::AuthChanged(
                        malus_protocol::wire::AuthStatusWire::with_message(
                            &p_id,
                            malus_protocol::wire::AuthStateWire::NeedsAuth,
                            message,
                        ),
                    ));
                }
                malus_protocol::provider::ProviderEvent::Error { code: _, message } => {
                    let _ = event_tx.send(ClientEvent::ProviderStateChanged {
                        provider: p_id.clone(),
                        state: format!("error: {message}"),
                    });
                }
            },
        );

        let proc_clone = proc.clone();
        tokio::spawn(async move {
            proc_clone.set_event_callback(cb).await;
        });
    }

    /// Register a discovered provider executable.
    pub async fn register_discovered(&self, provider: DiscoveredProvider) {
        let mut disc = self.discovered.write().await;
        disc.insert(provider.id.clone(), provider);
    }

    /// Register an already-running provider process (e.g. for testing).
    pub async fn register_provider(&self, provider: Arc<ProviderProcess>) {
        info!(
            "Registering running provider process: {} ({})",
            provider.name(),
            provider.id()
        );
        let id = provider.id().to_string();
        self.wire_provider_events(id.clone(), &provider);

        let mut map = self.running.write().await;
        map.insert(id.clone(), provider);

        let mut active = self.active_provider.write().await;
        if active.is_none() {
            *active = Some(id);
        }
    }

    /// Get a running provider process, or lazily spawn it if discovered.
    pub async fn get_or_spawn_provider(&self, id: &str) -> Result<Arc<ProviderProcess>, String> {
        // 1. Check if already running
        {
            let running = self.running.read().await;
            if let Some(p) = running.get(id)
                && p.is_ready().await
            {
                return Ok(p.clone());
            }
        }

        // 2. Check if discovered and needs lazy spawning
        let disc = {
            let discovered = self.discovered.read().await;
            discovered.get(id).cloned()
        };

        if let Some(dp) = disc {
            info!(
                "Lazily spawning provider '{}' from {}",
                dp.id,
                dp.executable.display()
            );
            let proc =
                ProviderProcess::spawn(&dp.id, format!("Provider {}", dp.id), &dp.executable, &[])
                    .await
                    .map_err(|e| format!("Failed to spawn provider '{}': {}", dp.id, e))?;

            let arc_proc = Arc::new(proc);
            self.wire_provider_events(id.to_string(), &arc_proc);
            let mut running = self.running.write().await;
            running.insert(id.to_string(), arc_proc.clone());

            let mut active = self.active_provider.write().await;
            if active.is_none() {
                *active = Some(id.to_string());
            }

            return Ok(arc_proc);
        }

        // 3. Check if present in running even if not marked ready
        {
            let running = self.running.read().await;
            if let Some(p) = running.get(id) {
                return Ok(p.clone());
            }
        }

        Err(format!("Provider '{id}' not found"))
    }

    /// Set the currently active audio provider.
    pub async fn set_active_provider(&self, id: &str) -> bool {
        if self.get_or_spawn_provider(id).await.is_ok() {
            let mut active = self.active_provider.write().await;
            *active = Some(id.to_string());
            true
        } else {
            false
        }
    }

    pub async fn get_active_provider(&self) -> Option<Arc<ProviderProcess>> {
        let active_id = {
            let active = self.active_provider.read().await;
            active.clone()
        };

        if let Some(id) = active_id {
            self.get_or_spawn_provider(&id).await.ok()
        } else {
            // If no active provider set, check if any discovered provider can become active
            let first_discovered = {
                let disc = self.discovered.read().await;
                disc.keys().next().cloned()
            };
            if let Some(id) = first_discovered {
                let proc = self.get_or_spawn_provider(&id).await.ok();
                if proc.is_some() {
                    let mut active = self.active_provider.write().await;
                    *active = Some(id);
                }
                proc
            } else {
                None
            }
        }
    }

    /// List all discovered and running providers without eagerly starting inactive ones.
    pub async fn list_providers(&self) -> Vec<ProviderInfoWire> {
        let mut ids: Vec<String> = Vec::new();
        {
            let disc = self.discovered.read().await;
            for k in disc.keys() {
                if !ids.contains(k) {
                    ids.push(k.clone());
                }
            }
        }
        {
            let running = self.running.read().await;
            for k in running.keys() {
                if !ids.contains(k) {
                    ids.push(k.clone());
                }
            }
        }
        ids.sort();

        let mut list = Vec::new();
        let running_map = self.running.read().await;

        for id in ids {
            if let Some(p) = running_map.get(&id) {
                let auth_state = p.auth_status().await.ok().map(|s| s.state);
                list.push(ProviderInfoWire {
                    id: id.clone(),
                    name: p.name().to_string(),
                    state: p.state().await.to_string(),
                    capabilities: p.capabilities().await,
                    auth_state,
                });
            } else {
                list.push(ProviderInfoWire {
                    id: id.clone(),
                    name: format!("Provider {id}"),
                    state: "stopped".to_string(),
                    capabilities: Vec::new(),
                    auth_state: None,
                });
            }
        }

        list
    }

    /// Emit an event to all subscribed clients.
    pub fn emit(&self, event: ClientEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to live engine and player events.
    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.event_tx.subscribe()
    }

    /// Handle an incoming client request and return a response.
    pub async fn handle_request(&self, req: ClientRequest) -> ClientResponse {
        match req {
            ClientRequest::Ping => ClientResponse::Pong,

            ClientRequest::GetCapabilities { provider } => {
                match self.get_or_spawn_provider(&provider).await {
                    Ok(p) => ClientResponse::Capabilities {
                        provider: p.id().to_string(),
                        capabilities: p.capabilities().await,
                    },
                    Err(e) => ClientResponse::err("PROVIDER_NOT_FOUND", e),
                }
            }

            ClientRequest::ListProviders => ClientResponse::Providers(self.list_providers().await),

            ClientRequest::GetAuthStatus { provider } => {
                match self.get_or_spawn_provider(&provider).await {
                    Ok(p) => match p.auth_status().await {
                        Ok(status) => ClientResponse::AuthStatus(status),
                        Err(e) => ClientResponse::err("AUTH_FAILED", e.to_string()),
                    },
                    Err(e) => ClientResponse::err("PROVIDER_NOT_FOUND", e),
                }
            }

            ClientRequest::AuthBegin { provider } => {
                match self.get_or_spawn_provider(&provider).await {
                    Ok(p) => match p.auth_begin().await {
                        Ok(status) => {
                            self.emit(ClientEvent::AuthChanged(status.clone()));
                            ClientResponse::AuthStatus(status)
                        }
                        Err(e) => ClientResponse::err("AUTH_FAILED", e.to_string()),
                    },
                    Err(e) => ClientResponse::err("PROVIDER_NOT_FOUND", e),
                }
            }

            ClientRequest::AuthLogout { provider } => {
                match self.get_or_spawn_provider(&provider).await {
                    Ok(p) => match p.auth_logout().await {
                        Ok(status) => {
                            self.emit(ClientEvent::AuthChanged(status.clone()));
                            ClientResponse::AuthStatus(status)
                        }
                        Err(e) => ClientResponse::err("AUTH_FAILED", e.to_string()),
                    },
                    Err(e) => ClientResponse::err("PROVIDER_NOT_FOUND", e),
                }
            }

            ClientRequest::Search { query } => {
                let active = self.get_active_provider().await;
                if let Some(p) = active {
                    match p.search(&query).await {
                        Ok(tracks) => ClientResponse::SearchResults { tracks },
                        Err(e) => {
                            warn!("Search on provider '{}' failed: {e}", p.id());
                            ClientResponse::SearchResults { tracks: Vec::new() }
                        }
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Play => {
                if let Some(p) = self.get_active_provider().await {
                    match p.resume().await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.update_mirrored_status(status.clone()).await;
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
                let target_provider = match malus_core::MediaId::parse(&media_id) {
                    Ok(mid) => Some(mid.provider().to_string()),
                    Err(_) => None,
                };
                let provider = if let Some(target) = &target_provider {
                    let _ = self.set_active_provider(target).await;
                    self.get_or_spawn_provider(target).await.ok()
                } else {
                    self.get_active_provider().await
                };

                if let Some(p) = provider {
                    match p.play_track(&media_id).await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.update_mirrored_status(status.clone()).await;
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
                                self.update_mirrored_status(status.clone()).await;
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
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.update_mirrored_status(status.clone()).await;
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
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
                                self.update_mirrored_status(status.clone()).await;
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
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.update_mirrored_status(status.clone()).await;
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
                        Err(e) => ClientResponse::err("NEXT_FAILED", e.to_string()),
                    }
                } else {
                    ClientResponse::err("NO_ACTIVE_PROVIDER", "No active audio provider")
                }
            }

            ClientRequest::Previous => {
                if let Some(p) = self.get_active_provider().await {
                    match p.previous().await {
                        Ok(()) => {
                            if let Ok(status) = p.get_status().await {
                                self.update_mirrored_status(status.clone()).await;
                                self.emit(ClientEvent::StatusChanged(status));
                            }
                            ClientResponse::Ok
                        }
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
                                self.update_mirrored_status(status.clone()).await;
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
                                self.update_mirrored_status(status.clone()).await;
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
                let cached = {
                    let guard = self.mirrored_player.read().await;
                    guard.as_ref().map(|m| m.extrapolated_status())
                };

                if let Some(status) = cached {
                    ClientResponse::Status(status)
                } else if let Some(p) = self.get_active_provider().await {
                    match p.get_status().await {
                        Ok(status) => {
                            let mut guard = self.mirrored_player.write().await;
                            *guard = Some(MirroredPlayerState::new(status.clone()));
                            ClientResponse::Status(status)
                        }
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
                match self.get_or_spawn_provider(&action_req.provider).await {
                    Ok(p) => {
                        match p
                            .custom_action(&action_req.action, action_req.target, action_req.params)
                            .await
                        {
                            Ok(result) => ClientResponse::ActionResult(result),
                            Err(e) => ClientResponse::err("ACTION_FAILED", e.to_string()),
                        }
                    }
                    Err(e) => ClientResponse::err("PROVIDER_NOT_FOUND", e),
                }
            }

            ClientRequest::SubscribeEvents => ClientResponse::Ok,
        }
    }

    /// Gracefully shut down all running provider processes.
    pub async fn shutdown(&self) {
        let running = self.running.read().await;
        for (id, p) in running.iter() {
            info!("Shutting down provider '{id}'...");
            let _ = p.shutdown().await;
        }
    }
}
