use crate::error::ClientError;
use malus_protocol::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    client::{ClientEvent, ClientRequest, ClientResponse},
    codec::{decode_message, read_frame, write_message},
    wire::{
        AuthStatusWire, CatalogItemWire, LibraryKindWire, LibraryPageWire, PageWire,
        PlayerStatusWire, ProviderInfoWire, ProviderSurfaceManifestWire, QueueWire, RepeatModeWire,
        SearchKindWire, SearchResultsWire, SurfaceActionResultWire, SurfaceContinuationWire,
        SurfaceCursorWire, SurfaceWire, TrackWire,
    },
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

    pub async fn get_status(&self) -> Result<PlayerStatusWire, ClientError> {
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

    pub async fn play_track(&self, media_id: &str) -> Result<(), ClientError> {
        match self
            .send(&ClientRequest::PlayTrack {
                media_id: media_id.to_string(),
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

    pub async fn set_repeat(&self, repeat: RepeatModeWire) -> Result<(), ClientError> {
        match self.send(&ClientRequest::SetRepeat { repeat }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn list_providers(&self) -> Result<Vec<ProviderInfoWire>, ClientError> {
        match self.send(&ClientRequest::ListProviders).await? {
            ClientResponse::Providers(list) => Ok(list),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_auth_status(&self, provider: &str) -> Result<AuthStatusWire, ClientError> {
        match self
            .send(&ClientRequest::GetAuthStatus {
                provider: provider.to_string(),
            })
            .await?
        {
            ClientResponse::AuthStatus(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn auth_begin(&self, provider: &str) -> Result<AuthStatusWire, ClientError> {
        match self
            .send(&ClientRequest::AuthBegin {
                provider: provider.to_string(),
            })
            .await?
        {
            ClientResponse::AuthStatus(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn auth_logout(&self, provider: &str) -> Result<AuthStatusWire, ClientError> {
        match self
            .send(&ClientRequest::AuthLogout {
                provider: provider.to_string(),
            })
            .await?
        {
            ClientResponse::AuthStatus(s) => Ok(s),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn search(
        &self,
        query: &str,
        kinds: Vec<SearchKindWire>,
        provider: Option<String>,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Result<SearchResultsWire, ClientError> {
        match self
            .send(&ClientRequest::Search {
                query: query.to_string(),
                kinds,
                provider,
                limit,
                cursor,
            })
            .await?
        {
            ClientResponse::SearchResults(results) => Ok(results),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_catalog_item(&self, media_id: &str) -> Result<CatalogItemWire, ClientError> {
        match self
            .send(&ClientRequest::GetCatalogItem {
                media_id: media_id.to_string(),
            })
            .await?
        {
            ClientResponse::CatalogItem(item) => Ok(item),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_collection_items(
        &self,
        media_id: &str,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Result<PageWire<TrackWire>, ClientError> {
        match self
            .send(&ClientRequest::GetCollectionItems {
                media_id: media_id.to_string(),
                limit,
                cursor,
            })
            .await?
        {
            ClientResponse::CollectionItems(page) => Ok(page),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_library(
        &self,
        kind: LibraryKindWire,
        provider: Option<String>,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Result<LibraryPageWire, ClientError> {
        match self
            .send(&ClientRequest::GetLibrary {
                kind,
                provider,
                limit,
                cursor,
            })
            .await?
        {
            ClientResponse::LibraryPage(page) => Ok(page),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_queue(&self) -> Result<QueueWire, ClientError> {
        match self.send(&ClientRequest::GetQueue).await? {
            ClientResponse::Queue(q) => Ok(q),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn clear_queue(&self) -> Result<(), ClientError> {
        match self.send(&ClientRequest::ClearQueue).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn enqueue(&self, track: TrackWire) -> Result<(), ClientError> {
        match self.send(&ClientRequest::Enqueue { track }).await? {
            ClientResponse::Ok => Ok(()),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_provider_surface_manifest(
        &self,
        provider: &str,
    ) -> Result<ProviderSurfaceManifestWire, ClientError> {
        match self
            .send(&ClientRequest::GetProviderSurfaceManifest {
                provider: provider.to_string(),
            })
            .await?
        {
            ClientResponse::ProviderSurfaceManifest(manifest) => Ok(manifest),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn get_surface(
        &self,
        provider: &str,
        surface_id: &str,
    ) -> Result<SurfaceWire, ClientError> {
        match self
            .send(&ClientRequest::GetSurface {
                provider: provider.to_string(),
                surface_id: surface_id.to_string(),
            })
            .await?
        {
            ClientResponse::Surface(surface) => Ok(surface),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn continue_surface(
        &self,
        provider: &str,
        surface_id: &str,
        cursor: SurfaceCursorWire,
    ) -> Result<SurfaceContinuationWire, ClientError> {
        match self
            .send(&ClientRequest::ContinueSurface {
                provider: provider.to_string(),
                surface_id: surface_id.to_string(),
                cursor,
            })
            .await?
        {
            ClientResponse::SurfaceContinued(cont) => Ok(cont),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }

    pub async fn invoke_surface_action(
        &self,
        provider: &str,
        invocation_token: &str,
    ) -> Result<SurfaceActionResultWire, ClientError> {
        match self
            .send(&ClientRequest::InvokeSurfaceAction {
                provider: provider.to_string(),
                invocation_token: invocation_token.to_string(),
            })
            .await?
        {
            ClientResponse::SurfaceActionResult(res) => Ok(res),
            other => Err(ClientError::UnexpectedResponse(Box::new(other))),
        }
    }
}
