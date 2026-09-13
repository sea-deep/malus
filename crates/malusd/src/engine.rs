//! Malus Daemon playback engine and coordinator for native Apple Music.
//!
//! Exposes fast in-process Apple Music operations via `malus-service` directly to
//! native client applications (CLI, TUI, GUI) over Unix domain socket IPC.

use malus_ipc::{
    client::{ClientEvent, ClientRequest, ClientResponse},
    wire::{ActionResultWire, PageActionWire},
};
use malus_model::{PlaybackState, PlayerStatus, Queue, RepeatMode};
use malus_service::AppleService;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::info;

#[derive(Debug, Clone)]
pub struct MirroredPlayerState {
    pub status: PlayerStatus,
    pub received_at: std::time::Instant,
}

impl MirroredPlayerState {
    pub fn new(status: PlayerStatus) -> Self {
        Self {
            status,
            received_at: std::time::Instant::now(),
        }
    }

    pub fn extrapolated_status(&self) -> PlayerStatus {
        let mut s = self.status.clone();
        if s.state == PlaybackState::Playing {
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

/// Central daemon engine coordinating Apple Music playback and metadata.
pub struct Engine {
    apple: Arc<AppleService>,
    mirrored_player: Arc<RwLock<Option<MirroredPlayerState>>>,
    mirrored_queue: Arc<RwLock<Queue>>,
    event_tx: broadcast::Sender<ClientEvent>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Initialize the engine with production Apple Music service.
    pub fn new() -> Self {
        let apple = Arc::new(AppleService::production());
        Self::with_apple(apple)
    }

    /// Initialize the engine with an explicit AppleService instance (test seam).
    pub fn with_apple(apple: Arc<AppleService>) -> Self {
        let (event_tx, _) = broadcast::channel(128);
        let mirrored_player = Arc::new(RwLock::new(None));
        let mirrored_queue = Arc::new(RwLock::new(Queue::new()));

        // Wire status events from Apple runtime directly into daemon
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        apple.set_event_sink(status_tx);

        let event_tx_clone = event_tx.clone();
        let mirrored_clone = mirrored_player.clone();
        let mirrored_queue_clone = mirrored_queue.clone();
        let apple_clone = apple.clone();

        tokio::spawn(async move {
            let mut last_track_id: Option<malus_model::MediaRef> = None;
            while let Some(status) = status_rx.recv().await {
                let track_changed =
                    status.current_track.as_ref().map(|t| &t.id) != last_track_id.as_ref();
                if track_changed {
                    last_track_id = status.current_track.as_ref().map(|t| t.id.clone());
                }

                {
                    let mut guard = mirrored_clone.write().await;
                    *guard = Some(MirroredPlayerState::new(status.clone()));
                }
                let _ = event_tx_clone.send(ClientEvent::StatusChanged(status));

                // Natural track transitions: refresh queue so current_index is in sync
                if track_changed && let Ok(queue) = apple_clone.get_queue().await {
                    let mut q_guard = mirrored_queue_clone.write().await;
                    *q_guard = queue.clone();
                    let _ = event_tx_clone.send(ClientEvent::QueueChanged(queue));
                }
            }
        });

        Self {
            apple,
            mirrored_player,
            mirrored_queue,
            event_tx,
        }
    }

    /// Reference to the underlying AppleService.
    pub fn apple(&self) -> &Arc<AppleService> {
        &self.apple
    }

    pub async fn update_mirrored_status(&self, status: PlayerStatus) {
        let mut guard = self.mirrored_player.write().await;
        *guard = Some(MirroredPlayerState::new(status));
    }

    pub async fn update_mirrored_queue(&self, queue: Queue) {
        let mut guard = self.mirrored_queue.write().await;
        *guard = queue;
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

            ClientRequest::GetAuthStatus => match self.apple.auth_status().await {
                Ok(status) => ClientResponse::AuthStatus(status),
                Err(e) => ClientResponse::err("AUTH_FAILED", e.to_string()),
            },

            ClientRequest::AuthBegin => match self.apple.auth_begin().await {
                Ok(status) => {
                    self.emit(ClientEvent::AuthChanged(status.clone()));
                    ClientResponse::AuthStatus(status)
                }
                Err(e) => ClientResponse::err("AUTH_FAILED", e.to_string()),
            },

            ClientRequest::AuthLogout => match self.apple.auth_logout().await {
                Ok(status) => {
                    self.emit(ClientEvent::AuthChanged(status.clone()));
                    ClientResponse::AuthStatus(status)
                }
                Err(e) => ClientResponse::err("AUTH_FAILED", e.to_string()),
            },

            ClientRequest::GetNavigation => ClientResponse::Navigation(self.apple.get_navigation()),

            ClientRequest::GetPage { route } => match self.apple.get_page(&route).await {
                Ok(page) => ClientResponse::Page(page),
                Err(e) => ClientResponse::err("PAGE_FAILED", e.to_string()),
            },

            ClientRequest::ContinuePage { route, cursor } => {
                match self.apple.continue_page(&route, &cursor).await {
                    Ok(cont) => ClientResponse::PageContinued(cont),
                    Err(e) => ClientResponse::err("PAGE_CONTINUE_FAILED", e.to_string()),
                }
            }

            ClientRequest::InvokeAction { action } => match action {
                PageActionWire::Play(reference) => match self.apple.play(&reference).await {
                    Ok(()) => {
                        if let Ok(status) = self.apple.get_status().await {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status));
                        }
                        if let Ok(queue) = self.apple.get_queue().await {
                            self.update_mirrored_queue(queue.clone()).await;
                            self.emit(ClientEvent::QueueChanged(queue));
                        }
                        ClientResponse::ActionResult(ActionResultWire::success())
                    }
                    Err(e) => ClientResponse::ActionResult(ActionResultWire::failed(e.to_string())),
                },
                PageActionWire::PlayNext(reference) => {
                    match self.apple.play_next(&reference).await {
                        Ok(()) => {
                            if let Ok(queue) = self.apple.get_queue().await {
                                self.update_mirrored_queue(queue.clone()).await;
                                self.emit(ClientEvent::QueueChanged(queue));
                            }
                            ClientResponse::ActionResult(ActionResultWire::success())
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::PlayLater(reference) => {
                    match self.apple.play_later(&reference).await {
                        Ok(()) => {
                            if let Ok(queue) = self.apple.get_queue().await {
                                self.update_mirrored_queue(queue.clone()).await;
                                self.emit(ClientEvent::QueueChanged(queue));
                            }
                            ClientResponse::ActionResult(ActionResultWire::success())
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::Favorite(reference) => {
                    match self.apple.favorite(&reference).await {
                        Ok(state) => {
                            self.emit(ClientEvent::MediaStateChanged(state));
                            ClientResponse::ActionResult(ActionResultWire::success())
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::Unfavorite(reference) => {
                    match self.apple.unfavorite(&reference).await {
                        Ok(state) => {
                            self.emit(ClientEvent::MediaStateChanged(state));
                            ClientResponse::ActionResult(ActionResultWire::success())
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::SuggestLess(reference) => {
                    match self.apple.suggest_less(&reference).await {
                        Ok(state) => {
                            self.emit(ClientEvent::MediaStateChanged(state));
                            ClientResponse::ActionResult(ActionResultWire::success())
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::AddToLibrary(reference) => {
                    match self.apple.add_to_library(&reference).await {
                        Ok(state) => {
                            self.emit(ClientEvent::MediaStateChanged(state));
                            ClientResponse::ActionResult(ActionResultWire::success())
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
            },

            ClientRequest::Search {
                query,
                kinds,
                limit,
                cursor,
            } => {
                let limit = limit.unwrap_or(20);
                match self
                    .apple
                    .search(&query, &kinds, limit, cursor.as_deref())
                    .await
                {
                    Ok(results) => ClientResponse::SearchResults(results),
                    Err(e) => ClientResponse::err("SEARCH_FAILED", e.to_string()),
                }
            }

            ClientRequest::GetCatalogItem { reference } => {
                match self.apple.get_catalog_item(&reference).await {
                    Ok(item) => ClientResponse::CatalogItem(item),
                    Err(e) => ClientResponse::err("CATALOG_FAILED", e.to_string()),
                }
            }

            ClientRequest::GetCollectionItems {
                reference,
                limit,
                cursor,
            } => {
                let limit = limit.unwrap_or(50);
                match self
                    .apple
                    .get_collection_items(&reference, limit, cursor.as_deref())
                    .await
                {
                    Ok(items) => ClientResponse::CollectionItems(items),
                    Err(e) => ClientResponse::err("COLLECTION_FAILED", e.to_string()),
                }
            }

            ClientRequest::GetLibrary {
                kind,
                limit,
                cursor,
            } => {
                let limit = limit.unwrap_or(50);
                match self.apple.get_library(kind, limit, cursor.as_deref()).await {
                    Ok(page) => ClientResponse::LibraryPage(page),
                    Err(e) => ClientResponse::err("LIBRARY_FAILED", e.to_string()),
                }
            }

            ClientRequest::Play => match self.apple.resume().await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("PLAY_FAILED", e.to_string()),
            },

            ClientRequest::PlayMedia { reference } => match self.apple.play(&reference).await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("PLAY_FAILED", e.to_string()),
            },

            ClientRequest::Pause => match self.apple.pause().await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("PAUSE_FAILED", e.to_string()),
            },

            ClientRequest::TogglePlay => {
                let current_state = {
                    let guard = self.mirrored_player.read().await;
                    guard
                        .as_ref()
                        .map(|m| m.status.state)
                        .unwrap_or(PlaybackState::Stopped)
                };

                let res = if current_state == PlaybackState::Playing {
                    self.apple.pause().await
                } else {
                    self.apple.resume().await
                };

                match res {
                    Ok(()) => {
                        if let Ok(status) = self.apple.get_status().await {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("TOGGLE_FAILED", e.to_string()),
                }
            }

            ClientRequest::Stop => match self.apple.stop().await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("STOP_FAILED", e.to_string()),
            },

            ClientRequest::Next => match self.apple.skip_to_next().await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("NEXT_FAILED", e.to_string()),
            },

            ClientRequest::Previous => match self.apple.skip_to_previous().await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("PREVIOUS_FAILED", e.to_string()),
            },

            ClientRequest::Seek { position_ms } => match self.apple.seek(position_ms).await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("SEEK_FAILED", e.to_string()),
            },

            ClientRequest::SetVolume { volume: _ } => ClientResponse::Ok,
            ClientRequest::SetShuffle { .. } => ClientResponse::Ok,
            ClientRequest::SetRepeat { .. } => ClientResponse::Ok,

            ClientRequest::GetStatus => {
                let cached = {
                    let guard = self.mirrored_player.read().await;
                    guard.as_ref().map(|m| m.extrapolated_status())
                };

                if let Some(status) = cached {
                    ClientResponse::Status(status)
                } else {
                    match self.apple.get_status().await {
                        Ok(status) => {
                            let mut guard = self.mirrored_player.write().await;
                            *guard = Some(MirroredPlayerState::new(status.clone()));
                            ClientResponse::Status(status)
                        }
                        Err(_) => ClientResponse::Status(PlayerStatus {
                            state: PlaybackState::Stopped,
                            current_track: None,
                            position_ms: 0,
                            duration_ms: 0,
                            volume: 100,
                            muted: false,
                            shuffle: false,
                            repeat: RepeatMode::Off,
                        }),
                    }
                }
            }

            ClientRequest::GetQueue => {
                let cached = self.mirrored_queue.read().await.clone();
                if !cached.items.is_empty() {
                    ClientResponse::Queue(cached)
                } else {
                    match self.apple.get_queue().await {
                        Ok(queue) => {
                            self.update_mirrored_queue(queue.clone()).await;
                            self.emit(ClientEvent::QueueChanged(queue.clone()));
                            ClientResponse::Queue(queue)
                        }
                        Err(e) => {
                            info!("GetQueue failed: {e}");
                            ClientResponse::Queue(Queue::new())
                        }
                    }
                }
            }

            ClientRequest::PlayNext { reference } => match self.apple.play_next(&reference).await {
                Ok(()) => {
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("PLAY_NEXT_FAILED", e.to_string()),
            },

            ClientRequest::PlayLater { reference } => {
                match self.apple.play_later(&reference).await {
                    Ok(()) => {
                        if let Ok(queue) = self.apple.get_queue().await {
                            self.update_mirrored_queue(queue.clone()).await;
                            self.emit(ClientEvent::QueueChanged(queue));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("PLAY_LATER_FAILED", e.to_string()),
                }
            }

            ClientRequest::QueueJump { index } => match self.apple.queue_jump(index).await {
                Ok(()) => {
                    if let Ok(status) = self.apple.get_status().await {
                        self.update_mirrored_status(status.clone()).await;
                        self.emit(ClientEvent::StatusChanged(status));
                    }
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("QUEUE_JUMP_FAILED", e.to_string()),
            },

            ClientRequest::QueueRemove { index } => match self.apple.queue_remove(index).await {
                Ok(()) => {
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("QUEUE_REMOVE_FAILED", e.to_string()),
            },

            ClientRequest::QueueMove { from, to } => match self.apple.queue_move(from, to).await {
                Ok(()) => {
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("QUEUE_MOVE_FAILED", e.to_string()),
            },

            ClientRequest::QueueClearUpcoming => match self.apple.queue_clear_upcoming().await {
                Ok(()) => {
                    if let Ok(queue) = self.apple.get_queue().await {
                        self.update_mirrored_queue(queue.clone()).await;
                        self.emit(ClientEvent::QueueChanged(queue));
                    }
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("QUEUE_CLEAR_UPCOMING_FAILED", e.to_string()),
            },

            ClientRequest::GetLyrics { reference } => {
                match self.apple.get_lyrics(&reference).await {
                    Ok(lyrics) => ClientResponse::Lyrics(lyrics),
                    Err(e) => ClientResponse::err("LYRICS_FAILED", e.to_string()),
                }
            }

            ClientRequest::GetCredits { reference } => {
                match self.apple.get_credits(&reference).await {
                    Ok(credits) => ClientResponse::Credits(credits),
                    Err(e) => ClientResponse::err("CREDITS_FAILED", e.to_string()),
                }
            }

            ClientRequest::GetMediaState { reference } => {
                match self.apple.get_account_media_state(&reference).await {
                    Ok(state) => ClientResponse::MediaState(state),
                    Err(e) => ClientResponse::err("GET_MEDIA_STATE_FAILED", e.to_string()),
                }
            }

            ClientRequest::Favorite { reference } => match self.apple.favorite(&reference).await {
                Ok(state) => {
                    self.emit(ClientEvent::MediaStateChanged(state.clone()));
                    ClientResponse::MediaState(state)
                }
                Err(e) => ClientResponse::err("FAVORITE_FAILED", e.to_string()),
            },

            ClientRequest::Unfavorite { reference } => {
                match self.apple.unfavorite(&reference).await {
                    Ok(state) => {
                        self.emit(ClientEvent::MediaStateChanged(state.clone()));
                        ClientResponse::MediaState(state)
                    }
                    Err(e) => ClientResponse::err("UNFAVORITE_FAILED", e.to_string()),
                }
            }

            ClientRequest::SuggestLess { reference } => {
                match self.apple.suggest_less(&reference).await {
                    Ok(state) => {
                        self.emit(ClientEvent::MediaStateChanged(state.clone()));
                        ClientResponse::MediaState(state)
                    }
                    Err(e) => ClientResponse::err("SUGGEST_LESS_FAILED", e.to_string()),
                }
            }

            ClientRequest::ClearRating { reference } => {
                match self.apple.clear_rating(&reference).await {
                    Ok(state) => {
                        self.emit(ClientEvent::MediaStateChanged(state.clone()));
                        ClientResponse::MediaState(state)
                    }
                    Err(e) => ClientResponse::err("CLEAR_RATING_FAILED", e.to_string()),
                }
            }

            ClientRequest::AddToLibrary { reference } => {
                match self.apple.add_to_library(&reference).await {
                    Ok(state) => {
                        self.emit(ClientEvent::MediaStateChanged(state.clone()));
                        ClientResponse::MediaState(state)
                    }
                    Err(e) => ClientResponse::err("ADD_TO_LIBRARY_FAILED", e.to_string()),
                }
            }

            ClientRequest::SubscribeEvents => ClientResponse::Ok,
        }
    }

    /// Gracefully shut down Apple Music service.
    pub async fn shutdown(&self) {
        info!("Shutting down Apple Music engine...");
        let _ = self.apple.shutdown().await;
    }
}
