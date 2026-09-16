//! Malus Daemon playback engine and coordinator for native Apple Music.
//!
//! Exposes fast in-process Apple Music operations via `malus-service` directly to
//! native client applications (CLI, TUI, GUI) over Unix domain socket IPC.

use malus_ipc::{
    client::{ClientEvent, ClientRequest, ClientResponse},
    wire::{ActionResultWire, PageActionWire},
};
use malus_model::{MediaRef, PlaybackState, PlayerStatus, Queue};
use malus_service::AppleService;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::info;

#[derive(Debug, Clone)]
pub struct MirroredPlayerState {
    pub status: PlayerStatus,
    pub received_at: std::time::Instant,
}

/// A bridge heartbeat arrives every ~1s. A sample older than this
/// threshold means the bridge is no longer delivering updates.
const STALE_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(3);

impl MirroredPlayerState {
    pub fn new(status: PlayerStatus) -> Self {
        Self {
            status,
            received_at: std::time::Instant::now(),
        }
    }

    /// Whether the cached sample is stale (bridge not delivering updates).
    pub fn is_stale(&self) -> bool {
        self.received_at.elapsed() > STALE_THRESHOLD
    }

    pub fn extrapolated_status(&self) -> PlayerStatus {
        let mut s = self.status.clone();
        if s.state == PlaybackState::Playing && !self.is_stale() {
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

/// Context tracking the origin of the current playback queue to detect
/// safe same-collection restarts and jumps without tearing down the pipeline.
#[derive(Debug, Clone, Default)]
pub struct QueueContext {
    pub origin: Option<MediaRef>,
    pub pristine: bool,
}

/// Central daemon engine coordinating Apple Music playback and metadata.
pub struct Engine {
    apple: Arc<AppleService>,
    mirrored_player: Arc<RwLock<Option<MirroredPlayerState>>>,
    mirrored_queue: Arc<RwLock<Queue>>,
    queue_context: Arc<RwLock<QueueContext>>,
    playback_mutex: Arc<tokio::sync::Mutex<()>>,
    event_tx: broadcast::Sender<ClientEvent>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    async fn finish_player_setting(
        &self,
        result: Result<(), malus_service::AppleError>,
    ) -> ClientResponse {
        match result {
            Ok(()) => match self.apple.get_status().await {
                Ok(status) => {
                    self.update_mirrored_status(status.clone()).await;
                    self.emit(ClientEvent::StatusChanged(status));
                    ClientResponse::Ok
                }
                Err(e) => ClientResponse::err("STATUS_FAILED", e.to_string()),
            },
            Err(e) => ClientResponse::err("PLAYER_SETTING_FAILED", e.to_string()),
        }
    }

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
        let queue_context = Arc::new(RwLock::new(QueueContext::default()));
        let playback_mutex = Arc::new(tokio::sync::Mutex::new(()));

        // Wire status events from Apple runtime directly into daemon
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        apple.set_event_sink(status_tx);

        let event_tx_clone = event_tx.clone();
        let mirrored_clone = mirrored_player.clone();
        let mirrored_queue_clone = mirrored_queue.clone();
        tokio::spawn(async move {
            while let Some(event) = status_rx.recv().await {
                match event {
                    malus_service::PlaybackEvent::Status(status) => {
                        *mirrored_clone.write().await =
                            Some(MirroredPlayerState::new(status.clone()));
                        let _ = event_tx_clone.send(ClientEvent::StatusChanged(status));
                    }
                    malus_service::PlaybackEvent::Queue(queue) => {
                        *mirrored_queue_clone.write().await = queue.clone();
                        let _ = event_tx_clone.send(ClientEvent::QueueChanged(queue));
                    }
                    malus_service::PlaybackEvent::Error { source, message } => {
                        let _ = event_tx_clone.send(ClientEvent::PlaybackError { source, message });
                    }
                }
            }
        });

        Self {
            apple,
            mirrored_player,
            mirrored_queue,
            queue_context,
            playback_mutex,
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

    /// Canonical media selection logic handling same-track restarts, same-collection jumps,
    /// and clean queue establishment without pipeline reconstruction stall.
    async fn play_media_internal(
        &self,
        reference: &MediaRef,
        collection: Option<MediaRef>,
        index: Option<usize>,
    ) -> Result<(), malus_service::AppleError> {
        let current_item_id = {
            let guard = self.mirrored_player.read().await;
            if let Some(m) = guard.as_ref().filter(|m| !m.is_stale()) {
                m.status.current_track.as_ref().map(|t| t.id.clone())
            } else {
                drop(guard);
                if let Ok(status) = self.apple.get_status().await {
                    self.update_mirrored_status(status.clone()).await;
                    status.current_track.as_ref().map(|t| t.id.clone())
                } else {
                    None
                }
            }
        };

        if let Some(coll) = collection {
            let q_ctx = self.queue_context.read().await.clone();
            let is_same_pristine = q_ctx.origin.as_ref() == Some(&coll) && q_ctx.pristine;
            if is_same_pristine {
                let cur_idx = self.mirrored_queue.read().await.current_index;
                if let Some(target_idx) = index {
                    if cur_idx == Some(target_idx) {
                        self.apple.restart_current_item().await
                    } else {
                        self.apple.queue_jump(target_idx).await
                    }
                } else if current_item_id.as_ref() == Some(reference) {
                    self.apple.restart_current_item().await
                } else {
                    let found_idx = self
                        .mirrored_queue
                        .read()
                        .await
                        .items
                        .iter()
                        .position(|t| &t.id == reference);
                    if let Some(idx) = found_idx {
                        self.apple.queue_jump(idx).await
                    } else {
                        self.apple.play(reference).await
                    }
                }
            } else {
                let res = match index {
                    Some(0) | None => self.apple.play_collection(&coll, false).await,
                    Some(idx) => self.apple.play_at_index(&coll, idx).await,
                };
                if res.is_ok() {
                    let mut ctx = self.queue_context.write().await;
                    ctx.origin = Some(coll);
                    ctx.pristine = true;
                }
                res
            }
        } else if matches!(
            reference,
            MediaRef::Album(_) | MediaRef::Playlist(_) | MediaRef::Station(_)
        ) {
            match index {
                Some(idx) => {
                    let coll = reference.clone();
                    let q_ctx = self.queue_context.read().await.clone();
                    let is_same_pristine = q_ctx.origin.as_ref() == Some(&coll) && q_ctx.pristine;
                    if is_same_pristine {
                        let cur_idx = self.mirrored_queue.read().await.current_index;
                        if cur_idx == Some(idx) {
                            self.apple.restart_current_item().await
                        } else {
                            self.apple.queue_jump(idx).await
                        }
                    } else {
                        let res = match idx {
                            0 => self.apple.play_collection(&coll, false).await,
                            _ => self.apple.play_at_index(&coll, idx).await,
                        };
                        if res.is_ok() {
                            let mut ctx = self.queue_context.write().await;
                            ctx.origin = Some(coll);
                            ctx.pristine = true;
                        }
                        res
                    }
                }
                None => self.play_collection_internal(reference, false).await,
            }
        } else {
            // Standalone track selection
            let q_ctx = self.queue_context.read().await.clone();
            let is_same_selection = current_item_id.as_ref() == Some(reference)
                || (q_ctx.origin.as_ref() == Some(reference) && q_ctx.pristine);
            if is_same_selection {
                self.apple.restart_current_item().await
            } else {
                let res = self.apple.play(reference).await;
                if res.is_ok() {
                    let mut ctx = self.queue_context.write().await;
                    ctx.origin = Some(reference.clone());
                    ctx.pristine = true;
                }
                res
            }
        }
    }

    /// Canonical collection selection logic handling same-collection restarts and jumps.
    async fn play_collection_internal(
        &self,
        reference: &MediaRef,
        shuffle: bool,
    ) -> Result<(), malus_service::AppleError> {
        if shuffle {
            let res = self.apple.play_collection(reference, true).await;
            if res.is_ok() {
                let mut ctx = self.queue_context.write().await;
                ctx.origin = Some(reference.clone());
                ctx.pristine = true;
            }
            res
        } else {
            let q_ctx = self.queue_context.read().await.clone();
            let is_same_pristine = q_ctx.origin.as_ref() == Some(reference) && q_ctx.pristine;
            if is_same_pristine {
                let cur_idx = self.mirrored_queue.read().await.current_index;
                if cur_idx == Some(0) {
                    self.apple.restart_current_item().await
                } else {
                    self.apple.queue_jump(0).await
                }
            } else {
                let res = self.apple.play_collection(reference, false).await;
                if res.is_ok() {
                    let mut ctx = self.queue_context.write().await;
                    ctx.origin = Some(reference.clone());
                    ctx.pristine = true;
                }
                res
            }
        }
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
                PageActionWire::Play(reference) => {
                    let _lock = self.playback_mutex.lock().await;
                    let res = match reference {
                        MediaRef::Album(_) | MediaRef::Playlist(_) => {
                            self.play_collection_internal(&reference, false).await
                        }
                        _ => self.play_media_internal(&reference, None, None).await,
                    };
                    match res {
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
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::PlayNext(reference) => {
                    let _lock = self.playback_mutex.lock().await;
                    match self.apple.play_next(&reference).await {
                        Ok(()) => {
                            {
                                let mut ctx = self.queue_context.write().await;
                                ctx.pristine = false;
                            }
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
                    let _lock = self.playback_mutex.lock().await;
                    match self.apple.play_later(&reference).await {
                        Ok(()) => {
                            {
                                let mut ctx = self.queue_context.write().await;
                                ctx.pristine = false;
                            }
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
                        Ok(mut state) => {
                            state.in_library = true;
                            self.emit(ClientEvent::MediaStateChanged(state));
                            let kind_str = match reference {
                                MediaRef::Song(_) => "Song",
                                MediaRef::Album(_) => "Album",
                                MediaRef::Playlist(_) => "Playlist",
                                _ => "Item",
                            };
                            ClientResponse::ActionResult(
                                ActionResultWire::success()
                                    .with_message(format!("Added {kind_str} to Library"))
                                    .with_refresh(malus_ipc::wire::PageRefreshWire::CurrentPage),
                            )
                        }
                        Err(e) => {
                            ClientResponse::ActionResult(ActionResultWire::failed(e.to_string()))
                        }
                    }
                }
                PageActionWire::RemoveFromLibrary(reference) => {
                    match self.apple.remove_from_library(&reference).await {
                        Ok(mut state) => {
                            state.in_library = false;
                            self.emit(ClientEvent::MediaStateChanged(state));
                            let kind_str = match reference {
                                MediaRef::Song(_) => "Song",
                                MediaRef::Album(_) => "Album",
                                MediaRef::Playlist(_) => "Playlist",
                                _ => "Item",
                            };
                            ClientResponse::ActionResult(
                                ActionResultWire::success()
                                    .with_message(format!("Removed {kind_str} from Library"))
                                    .with_refresh(malus_ipc::wire::PageRefreshWire::CurrentPage),
                            )
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

            ClientRequest::Play => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.resume().await {
                    Ok(()) => {
                        if let Ok(status) = self.apple.get_status().await {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("PLAY_FAILED", e.to_string()),
                }
            }

            ClientRequest::PlayMedia {
                reference,
                collection,
                index,
            } => {
                let _lock = self.playback_mutex.lock().await;
                match self
                    .play_media_internal(&reference, collection, index)
                    .await
                {
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
                }
            }

            ClientRequest::PlayCollection { reference, shuffle } => {
                let _lock = self.playback_mutex.lock().await;
                match self.play_collection_internal(&reference, shuffle).await {
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
                    Err(error) => ClientResponse::err("PLAY_FAILED", error.to_string()),
                }
            }

            ClientRequest::Pause => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.pause().await {
                    Ok(()) => {
                        if let Ok(status) = self.apple.get_status().await {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("PAUSE_FAILED", e.to_string()),
                }
            }

            ClientRequest::TogglePlay => {
                let _lock = self.playback_mutex.lock().await;
                let current_state = {
                    let guard = self.mirrored_player.read().await;
                    if let Some(m) = guard.as_ref().filter(|m| !m.is_stale()) {
                        m.status.state
                    } else {
                        drop(guard);
                        match self.apple.get_status().await {
                            Ok(status) => {
                                self.update_mirrored_status(status.clone()).await;
                                status.state
                            }
                            Err(_) => PlaybackState::Stopped,
                        }
                    }
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

            ClientRequest::Stop => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.stop().await {
                    Ok(()) => {
                        if let Ok(status) = self.apple.get_status().await {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("STOP_FAILED", e.to_string()),
                }
            }

            ClientRequest::Next => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.skip_to_next().await {
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
                }
            }

            ClientRequest::Previous => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.skip_to_previous().await {
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
                }
            }

            ClientRequest::Seek { position_ms } => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.seek(position_ms).await {
                    Ok(()) => {
                        if let Ok(status) = self.apple.get_status().await {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("SEEK_FAILED", e.to_string()),
                }
            }

            ClientRequest::SetVolume { volume } => {
                let _lock = self.playback_mutex.lock().await;
                self.finish_player_setting(self.apple.set_volume(volume).await)
                    .await
            }
            ClientRequest::SetShuffle { shuffle } => {
                let _lock = self.playback_mutex.lock().await;
                if shuffle {
                    let mut ctx = self.queue_context.write().await;
                    ctx.pristine = false;
                }
                self.finish_player_setting(self.apple.set_shuffle(shuffle).await)
                    .await
            }
            ClientRequest::SetRepeat { repeat } => {
                let _lock = self.playback_mutex.lock().await;
                self.finish_player_setting(self.apple.set_repeat(repeat).await)
                    .await
            }

            ClientRequest::GetStatus => {
                // Use cached status if fresh; fall through to live query if stale.
                let cached = {
                    let guard = self.mirrored_player.read().await;
                    guard
                        .as_ref()
                        .filter(|m| !m.is_stale())
                        .map(|m| m.extrapolated_status())
                };

                if let Some(status) = cached {
                    ClientResponse::Status(status)
                } else {
                    match self.apple.get_status().await {
                        Ok(status) => {
                            self.update_mirrored_status(status.clone()).await;
                            self.emit(ClientEvent::StatusChanged(status.clone()));
                            ClientResponse::Status(status)
                        }
                        Err(error) => ClientResponse::err("STATUS_FAILED", error.to_string()),
                    }
                }
            }

            ClientRequest::GetQueue => match self.apple.get_queue().await {
                Ok(queue) => {
                    self.update_mirrored_queue(queue.clone()).await;
                    ClientResponse::Queue(queue)
                }
                Err(error) => ClientResponse::err("QUEUE_FAILED", error.to_string()),
            },

            ClientRequest::QueueJump { index } => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.queue_jump(index).await {
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
                }
            }

            ClientRequest::QueueRemove { index } => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.queue_remove(index).await {
                    Ok(()) => {
                        {
                            let mut ctx = self.queue_context.write().await;
                            ctx.pristine = false;
                        }
                        if let Ok(queue) = self.apple.get_queue().await {
                            self.update_mirrored_queue(queue.clone()).await;
                            self.emit(ClientEvent::QueueChanged(queue));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("QUEUE_REMOVE_FAILED", e.to_string()),
                }
            }

            ClientRequest::QueueMove { from, to } => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.queue_move(from, to).await {
                    Ok(()) => {
                        {
                            let mut ctx = self.queue_context.write().await;
                            ctx.pristine = false;
                        }
                        if let Ok(queue) = self.apple.get_queue().await {
                            self.update_mirrored_queue(queue.clone()).await;
                            self.emit(ClientEvent::QueueChanged(queue));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("QUEUE_MOVE_FAILED", e.to_string()),
                }
            }

            ClientRequest::QueueClearUpcoming => {
                let _lock = self.playback_mutex.lock().await;
                match self.apple.queue_clear_upcoming().await {
                    Ok(()) => {
                        {
                            let mut ctx = self.queue_context.write().await;
                            ctx.pristine = false;
                        }
                        if let Ok(queue) = self.apple.get_queue().await {
                            self.update_mirrored_queue(queue.clone()).await;
                            self.emit(ClientEvent::QueueChanged(queue));
                        }
                        ClientResponse::Ok
                    }
                    Err(e) => ClientResponse::err("QUEUE_CLEAR_UPCOMING_FAILED", e.to_string()),
                }
            }

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
                    Ok(mut state) => {
                        state.in_library = true;
                        self.emit(ClientEvent::MediaStateChanged(state.clone()));
                        ClientResponse::MediaState(state)
                    }
                    Err(e) => ClientResponse::err("ADD_TO_LIBRARY_FAILED", e.to_string()),
                }
            }

            ClientRequest::RemoveFromLibrary { reference } => {
                match self.apple.remove_from_library(&reference).await {
                    Ok(mut state) => {
                        state.in_library = false;
                        self.emit(ClientEvent::MediaStateChanged(state.clone()));
                        ClientResponse::MediaState(state)
                    }
                    Err(e) => ClientResponse::err("REMOVE_FROM_LIBRARY_FAILED", e.to_string()),
                }
            }

            ClientRequest::CreatePlaylist {
                name,
                description,
                initial_tracks,
            } => {
                match self
                    .apple
                    .create_playlist(&name, description.as_deref(), &initial_tracks)
                    .await
                {
                    Ok(playlist) => ClientResponse::Playlist(playlist),
                    Err(e) => ClientResponse::err("CREATE_PLAYLIST_FAILED", e.to_string()),
                }
            }

            ClientRequest::AddTracksToPlaylist { playlist, tracks } => {
                match self.apple.add_tracks_to_playlist(&playlist, &tracks).await {
                    Ok(()) => ClientResponse::Ok,
                    Err(e) => ClientResponse::err("ADD_TRACKS_FAILED", e.to_string()),
                }
            }

            ClientRequest::RemoveTrackFromPlaylist {
                playlist,
                track_index,
                expected_track,
            } => {
                match self
                    .apple
                    .remove_track_from_playlist(&playlist, track_index, &expected_track)
                    .await
                {
                    Ok(()) => ClientResponse::Ok,
                    Err(e) => ClientResponse::err("REMOVE_TRACK_FAILED", e.to_string()),
                }
            }

            ClientRequest::UpdatePlaylist {
                playlist,
                name,
                description,
            } => {
                match self
                    .apple
                    .update_playlist(&playlist, &name, description.as_deref())
                    .await
                {
                    Ok(()) => ClientResponse::Ok,
                    Err(e) => ClientResponse::err("UPDATE_PLAYLIST_FAILED", e.to_string()),
                }
            }

            ClientRequest::DeletePlaylist { playlist } => {
                match self.apple.delete_playlist(&playlist).await {
                    Ok(()) => ClientResponse::Ok,
                    Err(e) => ClientResponse::err("DELETE_PLAYLIST_FAILED", e.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use malus_model::RepeatMode;
    use std::time::{Duration, Instant};

    fn playing_status(pos_ms: u64, dur_ms: u64) -> PlayerStatus {
        PlayerStatus {
            state: PlaybackState::Playing,
            current_track: None,
            position_ms: pos_ms,
            duration_ms: dur_ms,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
        }
    }

    fn paused_status(pos_ms: u64, dur_ms: u64) -> PlayerStatus {
        PlayerStatus {
            state: PlaybackState::Paused,
            current_track: None,
            position_ms: pos_ms,
            duration_ms: dur_ms,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
        }
    }

    #[test]
    fn fresh_playing_extrapolates() {
        let state = MirroredPlayerState {
            status: playing_status(5000, 200000),
            received_at: Instant::now() - Duration::from_millis(500),
        };
        assert!(!state.is_stale());
        let ext = state.extrapolated_status();
        assert_eq!(ext.state, PlaybackState::Playing);
        // Should have advanced by ~500ms
        assert!(ext.position_ms >= 5400 && ext.position_ms <= 5700);
    }

    #[test]
    fn stale_playing_does_not_extrapolate() {
        let state = MirroredPlayerState {
            status: playing_status(5000, 200000),
            received_at: Instant::now() - Duration::from_secs(5),
        };
        assert!(state.is_stale());
        let ext = state.extrapolated_status();
        // State still reports Playing, but position is frozen at the raw value
        assert_eq!(ext.state, PlaybackState::Playing);
        assert_eq!(ext.position_ms, 5000);
    }

    #[test]
    fn paused_does_not_extrapolate() {
        let state = MirroredPlayerState {
            status: paused_status(5000, 200000),
            received_at: Instant::now() - Duration::from_millis(500),
        };
        let ext = state.extrapolated_status();
        assert_eq!(ext.state, PlaybackState::Paused);
        assert_eq!(ext.position_ms, 5000);
    }

    #[test]
    fn extrapolation_capped_at_duration() {
        let state = MirroredPlayerState {
            status: playing_status(199500, 200000),
            received_at: Instant::now() - Duration::from_secs(2),
        };
        assert!(!state.is_stale());
        let ext = state.extrapolated_status();
        assert_eq!(ext.position_ms, 200000); // Capped at duration
    }

    #[test]
    fn stale_threshold_boundary() {
        // Exactly at 3s — should still be within tolerance
        let just_under = MirroredPlayerState {
            status: playing_status(1000, 200000),
            received_at: Instant::now() - Duration::from_millis(2999),
        };
        assert!(!just_under.is_stale());

        let just_over = MirroredPlayerState {
            status: playing_status(1000, 200000),
            received_at: Instant::now() - Duration::from_millis(3001),
        };
        assert!(just_over.is_stale());
    }
}
