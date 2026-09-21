use crate::error::ClientError;
use malus_ipc::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    client::{ClientEvent, ClientRequest, ClientResponse},
    codec::{decode_message, read_frame, write_message},
    wire::{
        ActionResultWire, AuthStatusWire, CatalogItemWire, LibraryKindWire, LibraryPageWire,
        NavigationWire, PageActionWire, PageContinuationWire, PageCursorWire, PageWire,
        PagedListWire, SearchKindWire, SearchResultsWire, SearchScopeWire,
    },
};
use malus_model::{
    AccountMediaState, Credits, Lyrics, MediaRef, PageRoute, PlayerStatus, Queue, RepeatMode, Track,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::{broadcast, watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

/// Asynchronous client for communicating with the `malusd` daemon over a Unix domain socket.
///
/// Uses independent connections for requests to prevent head-of-line blocking,
/// and a dedicated background connection with automatic reconnection for event streaming.
#[derive(Clone, Debug)]
pub struct MalusClient {
    socket_path: PathBuf,
}

impl MalusClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Verifies connectivity to the daemon at `socket_path` by sending a Ping request.
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let path = socket_path.as_ref().to_path_buf();
        let client = Self::new(path);
        client.ping().await?;
        Ok(client)
    }

    /// Send a request and wait for the daemon's response using a default 60-second timeout.
    pub async fn send(&self, request: &ClientRequest) -> Result<ClientResponse, ClientError> {
        self.send_timeout(request, Duration::from_secs(60)).await
    }

    /// Send a request and wait for the daemon's response with a custom timeout.
    /// Opens an independent Unix socket connection per request so slow/cold requests
    /// do not block concurrent requests.
    pub async fn send_timeout(
        &self,
        request: &ClientRequest,
        timeout: Duration,
    ) -> Result<ClientResponse, ClientError> {
        tokio::time::timeout(timeout, async {
            let stream = UnixStream::connect(&self.socket_path)
                .await
                .map_err(|e| ClientError::ConnectionFailed(self.socket_path.clone(), e))?;
            let (mut reader, mut writer) = stream.into_split();
            write_message(&mut writer, request).await?;
            let mut buf = Vec::new();
            match read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await? {
                Some(frame) => {
                    let resp: ClientResponse = decode_message(&frame)?;
                    if let ClientResponse::Error { code, message } = resp {
                        Err(ClientError::ServerError { code, message })
                    } else {
                        Ok(resp)
                    }
                }
                None => Err(ClientError::Disconnected),
            }
        })
        .await
        .map_err(|_| ClientError::Timeout(timeout))?
    }

    /// Enter subscription mode on a dedicated connection and read events continuously via callback.
    /// Used by CLI commands for simple single-connection event streaming.
    pub async fn stream_events<F>(&mut self, mut on_event: F) -> Result<(), ClientError>
    where
        F: FnMut(ClientEvent),
    {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| ClientError::ConnectionFailed(self.socket_path.clone(), e))?;
        let (mut reader, mut writer) = stream.into_split();
        let mut buf = Vec::new();

        write_message(&mut writer, &ClientRequest::SubscribeEvents).await?;
        match read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await? {
            Some(frame) => {
                let resp: ClientResponse = decode_message(&frame)?;
                if resp != ClientResponse::Ok {
                    return Err(ClientError::UnexpectedResponse(Box::new(resp)));
                }
            }
            None => return Err(ClientError::Disconnected),
        }

        while let Some(frame) = read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await?
        {
            match decode_message(&frame) {
                Ok(event) => on_event(event),
                Err(e) => tracing::warn!("Failed to decode event: {e}"),
            }
        }

        Ok(())
    }

    /// Subscribes to daemon events on a background task with automatic reconnection and exponential backoff.
    /// Returns a broadcast receiver for events and a watch receiver for connection status.
    pub fn subscribe_events(
        &self,
    ) -> (
        broadcast::Receiver<ClientEvent>,
        watch::Receiver<ConnectionStatus>,
    ) {
        let (event_tx, event_rx) = broadcast::channel(256);
        let (status_tx, status_rx) = watch::channel(ConnectionStatus::Connecting);
        let socket_path = self.socket_path.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_millis(500);
            loop {
                let _ = status_tx.send(ConnectionStatus::Connecting);
                match UnixStream::connect(&socket_path).await {
                    Ok(stream) => {
                        let (mut reader, mut writer) = stream.into_split();
                        let mut buf = Vec::new();
                        if let Err(e) =
                            write_message(&mut writer, &ClientRequest::SubscribeEvents).await
                        {
                            tracing::debug!("SubscribeEvents write failed: {e}");
                            let _ = status_tx.send(ConnectionStatus::Disconnected);
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(5));
                            continue;
                        }
                        match read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await {
                            Ok(Some(frame)) => match decode_message::<ClientResponse>(&frame) {
                                Ok(ClientResponse::Ok) => {
                                    let _ = status_tx.send(ConnectionStatus::Connected);
                                    backoff = Duration::from_millis(500);
                                }
                                _ => {
                                    let _ = status_tx.send(ConnectionStatus::Disconnected);
                                    tokio::time::sleep(backoff).await;
                                    backoff = (backoff * 2).min(Duration::from_secs(5));
                                    continue;
                                }
                            },
                            _ => {
                                let _ = status_tx.send(ConnectionStatus::Disconnected);
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(Duration::from_secs(5));
                                continue;
                            }
                        }

                        // Connected and subscribed, enter read loop
                        loop {
                            match read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await
                            {
                                Ok(Some(frame)) => match decode_message::<ClientEvent>(&frame) {
                                    Ok(event) => {
                                        let _ = event_tx.send(event);
                                    }
                                    Err(e) => tracing::warn!("Failed to decode event: {e}"),
                                },
                                Ok(None) => {
                                    tracing::info!("Daemon closed event connection");
                                    break;
                                }
                                Err(e) => {
                                    tracing::warn!("Error reading event frame: {e}");
                                    break;
                                }
                            }
                        }
                        let _ = status_tx.send(ConnectionStatus::Disconnected);
                    }
                    Err(_) => {
                        let _ = status_tx.send(ConnectionStatus::Disconnected);
                    }
                }

                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
        });

        (event_rx, status_rx)
    }

    // --- Typed helper methods ---

    pub async fn ping(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Ping).await? {
            ClientResponse::Pong => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_status(&self) -> Result<PlayerStatus, ClientError> {
        match self.send(&ClientRequest::GetStatus).await? {
            ClientResponse::Status(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn play(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Play).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn play_media(&self, reference: &MediaRef) -> Result<(), ClientError> {
        self.play_media_with_context(reference, None, None).await
    }

    pub async fn play_media_with_context(
        &self,
        reference: &MediaRef,
        collection: Option<&MediaRef>,
        index: Option<usize>,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::PlayMedia {
                reference: reference.clone(),
                collection: collection.cloned(),
                index,
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn play_collection(
        &self,
        reference: &MediaRef,
        shuffle: bool,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::PlayCollection {
                reference: reference.clone(),
                shuffle,
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn pause(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Pause).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn toggle_play(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::TogglePlay).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn stop(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Stop).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn next(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Next).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn previous(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Previous).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn seek(&self, position_ms: u64) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Seek { position_ms }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn set_volume(&self, volume: u8) -> Result<(), ClientError> {
        match self.send(&ClientRequest::SetVolume { volume }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn set_shuffle(&self, shuffle: bool) -> Result<(), ClientError> {
        match self.send(&ClientRequest::SetShuffle { shuffle }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn set_repeat(&self, repeat: RepeatMode) -> Result<(), ClientError> {
        match self.send(&ClientRequest::SetRepeat { repeat }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn set_autoplay(&self, autoplay: bool) -> Result<(), ClientError> {
        match self.send(&ClientRequest::SetAutoplay { autoplay }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_auth_status(&self) -> Result<AuthStatusWire, ClientError> {
        match self.send(&ClientRequest::GetAuthStatus).await? {
            ClientResponse::AuthStatus(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn auth_begin(&self) -> Result<AuthStatusWire, ClientError> {
        match self.send(&ClientRequest::AuthBegin).await? {
            ClientResponse::AuthStatus(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn auth_logout(&self) -> Result<AuthStatusWire, ClientError> {
        match self.send(&ClientRequest::AuthLogout).await? {
            ClientResponse::AuthStatus(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn search(
        &self,
        query: &str,
        kinds: Vec<SearchKindWire>,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Result<SearchResultsWire, ClientError> {
        self.search_with_scope(query, kinds, limit, cursor, SearchScopeWire::Catalog)
            .await
    }

    pub async fn search_with_scope(
        &self,
        query: &str,
        kinds: Vec<SearchKindWire>,
        limit: Option<usize>,
        cursor: Option<String>,
        scope: SearchScopeWire,
    ) -> Result<SearchResultsWire, ClientError> {
        match self
            .send(&ClientRequest::Search {
                query: query.to_string(),
                kinds,
                limit,
                cursor,
                scope: Some(scope),
            })
            .await?
        {
            ClientResponse::SearchResults(results) => Ok(results),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_catalog_item(
        &self,
        reference: &MediaRef,
    ) -> Result<CatalogItemWire, ClientError> {
        match self
            .send(&ClientRequest::GetCatalogItem {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::CatalogItem(item) => Ok(item),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_collection_items(
        &self,
        reference: &MediaRef,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Result<PagedListWire<Track>, ClientError> {
        match self
            .send(&ClientRequest::GetCollectionItems {
                reference: reference.clone(),
                limit,
                cursor,
            })
            .await?
        {
            ClientResponse::CollectionItems(items) => Ok(items),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_library(
        &self,
        kind: LibraryKindWire,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Result<LibraryPageWire, ClientError> {
        match self
            .send(&ClientRequest::GetLibrary {
                kind,
                limit,
                cursor,
            })
            .await?
        {
            ClientResponse::LibraryPage(page) => Ok(page),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_queue(&self) -> Result<Queue, ClientError> {
        match self.send(&ClientRequest::GetQueue).await? {
            ClientResponse::Queue(q) => Ok(q),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn play_next(&self, reference: &MediaRef) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::InvokeAction {
                action: PageActionWire::PlayNext(reference.clone()),
            })
            .await?
        {
            ClientResponse::ActionResult(res) => {
                if res.is_success() {
                    Ok(())
                } else {
                    Err(ClientError::ServerError {
                        code: "PLAY_NEXT_FAILED".to_string(),
                        message: res
                            .message
                            .unwrap_or_else(|| "Failed to play next".to_string()),
                    })
                }
            }
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn play_later(&self, reference: &MediaRef) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::InvokeAction {
                action: PageActionWire::PlayLater(reference.clone()),
            })
            .await?
        {
            ClientResponse::ActionResult(res) => {
                if res.is_success() {
                    Ok(())
                } else {
                    Err(ClientError::ServerError {
                        code: "PLAY_LATER_FAILED".to_string(),
                        message: res
                            .message
                            .unwrap_or_else(|| "Failed to play later".to_string()),
                    })
                }
            }
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn queue_jump(&self, index: usize) -> Result<(), ClientError> {
        self.queue_jump_checked(index, None).await
    }

    pub async fn queue_jump_checked(
        &self,
        index: usize,
        expected_id: Option<String>,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::QueueJump { index, expected_id })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn queue_remove(&self, index: usize) -> Result<(), ClientError> {
        self.queue_remove_checked(index, None).await
    }

    pub async fn queue_remove_checked(
        &self,
        index: usize,
        expected_id: Option<String>,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::QueueRemove { index, expected_id })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn queue_move(&self, from: usize, to: usize) -> Result<(), ClientError> {
        self.queue_move_checked(from, to, None).await
    }

    pub async fn queue_move_checked(
        &self,
        from: usize,
        to: usize,
        expected_id: Option<String>,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::QueueMove {
                from,
                to,
                expected_id,
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn queue_clear_upcoming(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::QueueClearUpcoming).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_navigation(&self) -> Result<NavigationWire, ClientError> {
        match self.send(&ClientRequest::GetNavigation).await? {
            ClientResponse::Navigation(nav) => Ok(nav),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_page(&self, route: &PageRoute) -> Result<PageWire, ClientError> {
        match self
            .send(&ClientRequest::GetPage {
                route: route.clone(),
            })
            .await?
        {
            ClientResponse::Page(page) => Ok(page),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn continue_page(
        &self,
        route: &PageRoute,
        cursor: PageCursorWire,
    ) -> Result<PageContinuationWire, ClientError> {
        match self
            .send(&ClientRequest::ContinuePage {
                route: route.clone(),
                cursor,
            })
            .await?
        {
            ClientResponse::PageContinued(cont) => Ok(cont),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn invoke_action(
        &self,
        action: PageActionWire,
    ) -> Result<ActionResultWire, ClientError> {
        match self.send(&ClientRequest::InvokeAction { action }).await? {
            ClientResponse::ActionResult(res) => Ok(res),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_lyrics(&self, reference: &MediaRef) -> Result<Lyrics, ClientError> {
        match self
            .send(&ClientRequest::GetLyrics {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::Lyrics(lyrics) => Ok(lyrics),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_credits(&self, reference: &MediaRef) -> Result<Credits, ClientError> {
        match self
            .send(&ClientRequest::GetCredits {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::Credits(credits) => Ok(credits),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_media_state(
        &self,
        reference: &MediaRef,
    ) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::GetMediaState {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn favorite(&self, reference: &MediaRef) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::Favorite {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn unfavorite(&self, reference: &MediaRef) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::Unfavorite {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn suggest_less(
        &self,
        reference: &MediaRef,
    ) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::SuggestLess {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn clear_rating(
        &self,
        reference: &MediaRef,
    ) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::ClearRating {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn add_to_library(
        &self,
        reference: &MediaRef,
    ) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::AddToLibrary {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn remove_from_library(
        &self,
        reference: &MediaRef,
    ) -> Result<AccountMediaState, ClientError> {
        match self
            .send(&ClientRequest::RemoveFromLibrary {
                reference: reference.clone(),
            })
            .await?
        {
            ClientResponse::MediaState(state) => Ok(state),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn create_playlist(
        &self,
        name: impl Into<String>,
        description: Option<String>,
        initial_tracks: Vec<MediaRef>,
    ) -> Result<malus_model::Playlist, ClientError> {
        match self
            .send(&ClientRequest::CreatePlaylist {
                name: name.into(),
                description,
                initial_tracks,
            })
            .await?
        {
            ClientResponse::Playlist(playlist) => Ok(playlist),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn add_tracks_to_playlist(
        &self,
        playlist: &MediaRef,
        tracks: Vec<MediaRef>,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::AddTracksToPlaylist {
                playlist: playlist.clone(),
                tracks,
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn remove_track_from_playlist(
        &self,
        playlist: &MediaRef,
        track_index: usize,
        expected_track: &MediaRef,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::RemoveTrackFromPlaylist {
                playlist: playlist.clone(),
                track_index,
                expected_track: expected_track.clone(),
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn update_playlist(
        &self,
        playlist: &MediaRef,
        name: impl Into<String>,
        description: Option<String>,
    ) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::UpdatePlaylist {
                playlist: playlist.clone(),
                name: name.into(),
                description,
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn delete_playlist(&self, playlist: &MediaRef) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::DeletePlaylist {
                playlist: playlist.clone(),
            })
            .await?
        {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }
}
