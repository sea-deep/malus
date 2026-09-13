//! Standard I/O process server for Malus audio providers.
//!
//! Exposes a convenient server loop that reads LSP Content-Length framed JSON
//! requests from standard input, calls provider methods, and writes framed JSON
//! responses to standard output.
//!
//! Standard error (`stderr`) is reserved for logging and diagnostic output.

use crate::traits::Provider;
use malus_protocol::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    codec::{decode_message, read_frame, write_message},
    provider::{
        PROVIDER_PROTOCOL, ProviderEvent, ProviderRequest, ProviderRequestEnvelope,
        ProviderResponse, ProviderWireMessage,
    },
};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

/// Serve a provider over an arbitrary async reader and writer.
pub async fn serve_io<R: AsyncRead + Unpin, W: AsyncWrite + Unpin + Send + 'static>(
    provider: Arc<dyn Provider>,
    mut reader: R,
    mut writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<ProviderWireMessage>();

    // Register event sink
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ProviderEvent>();
    provider.register_event_sink(event_tx);

    // Dedicated writer task: serializes all outgoing responses and events atomically
    let writer_handle = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if write_message(&mut writer, &msg).await.is_err() {
                break;
            }
        }
    });

    // Dedicated event forwarder: forwards ProviderEvent into ProviderWireMessage::Event
    let msg_tx_events = msg_tx.clone();
    let event_fwd_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if msg_tx_events
                .send(ProviderWireMessage::event(event))
                .is_err()
            {
                break;
            }
        }
    });

    let mut buf = Vec::new();
    while let Some(frame) = read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await? {
        let req_env: ProviderRequestEnvelope = match decode_message(&frame) {
            Ok(r) => r,
            Err(e) => {
                let err_res = ProviderWireMessage::response(
                    0,
                    ProviderResponse::err("INVALID_REQUEST", e.to_string()),
                );
                let _ = msg_tx.send(err_res);
                continue;
            }
        };

        let req_id = req_env.id;
        let is_shutdown = matches!(req_env.request, ProviderRequest::Shutdown);
        let resp = handle_request(&*provider, req_env.request).await;
        let _ = msg_tx.send(ProviderWireMessage::response(req_id, resp));

        if is_shutdown {
            break;
        }
    }

    drop(msg_tx);
    event_fwd_handle.abort();
    let _ = writer_handle.await;

    Ok(())
}

/// Serve a provider over standard input and standard output.
pub async fn serve(provider: Arc<dyn Provider>) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    serve_io(provider, stdin, stdout).await
}

async fn handle_request(provider: &dyn Provider, req: ProviderRequest) -> ProviderResponse {
    match req {
        ProviderRequest::Ping => ProviderResponse::Pong,

        ProviderRequest::Hello { version: _ } => ProviderResponse::Hello {
            id: provider.id().to_string(),
            name: provider.name().to_string(),
            version: PROVIDER_PROTOCOL,
            capabilities: provider.capabilities(),
            status: "ready".to_string(),
        },

        ProviderRequest::GetCapabilities => ProviderResponse::Capabilities(provider.capabilities()),

        ProviderRequest::GetSurfaceManifest => match provider.get_surface_manifest().await {
            Ok(manifest) => ProviderResponse::SurfaceManifest(manifest),
            Err(e) => ProviderResponse::err("SURFACE_MANIFEST_FAILED", e.to_string()),
        },

        ProviderRequest::GetSurface { surface_id } => {
            match provider.get_surface(&surface_id).await {
                Ok(surface) => ProviderResponse::Surface(surface),
                Err(e) => ProviderResponse::err("SURFACE_FAILED", e.to_string()),
            }
        }

        ProviderRequest::ContinueSurface { surface_id, cursor } => {
            match provider.continue_surface(&surface_id, &cursor).await {
                Ok(cont) => ProviderResponse::SurfaceContinued(cont),
                Err(e) => ProviderResponse::err("SURFACE_CONTINUE_FAILED", e.to_string()),
            }
        }

        ProviderRequest::InvokeSurfaceAction { invocation_token } => {
            match provider.invoke_surface_action(&invocation_token).await {
                Ok(res) => ProviderResponse::SurfaceActionResult(res),
                Err(e) => ProviderResponse::err("SURFACE_ACTION_FAILED", e.to_string()),
            }
        }

        ProviderRequest::Search {
            query,
            kinds,
            limit,
            cursor,
        } => match provider
            .search(&query, &kinds, limit, cursor.as_deref())
            .await
        {
            Ok(results) => ProviderResponse::SearchResults(results),
            Err(e) => ProviderResponse::err("SEARCH_FAILED", e.to_string()),
        },

        ProviderRequest::GetCatalogItem { media_id } => {
            match provider.get_catalog_item(&media_id).await {
                Ok(item) => ProviderResponse::CatalogItem(item),
                Err(e) => ProviderResponse::err("CATALOG_FAILED", e.to_string()),
            }
        }

        ProviderRequest::GetCollectionItems {
            media_id,
            limit,
            cursor,
        } => {
            match provider
                .get_collection_items(&media_id, limit, cursor.as_deref())
                .await
            {
                Ok(page) => ProviderResponse::CollectionItems(page),
                Err(e) => ProviderResponse::err("COLLECTION_FAILED", e.to_string()),
            }
        }

        ProviderRequest::GetLibrary {
            kind,
            limit,
            cursor,
        } => match provider.get_library(kind, limit, cursor.as_deref()).await {
            Ok(page) => ProviderResponse::LibraryPage(page),
            Err(e) => ProviderResponse::err("LIBRARY_FAILED", e.to_string()),
        },

        ProviderRequest::Play => match provider.resume().await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("PLAY_FAILED", e.to_string()),
        },

        ProviderRequest::PlayTrack { media_id } => match provider.play(&media_id).await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("PLAY_FAILED", e.to_string()),
        },

        ProviderRequest::Pause => match provider.pause().await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("PAUSE_FAILED", e.to_string()),
        },

        ProviderRequest::TogglePlay => match provider.get_status().await {
            Ok(status) => {
                let res = if status.state == malus_protocol::PlaybackStateWire::Playing {
                    provider.pause().await
                } else {
                    provider.resume().await
                };
                match res {
                    Ok(()) => ProviderResponse::Ok,
                    Err(e) => ProviderResponse::err("TOGGLE_FAILED", e.to_string()),
                }
            }
            Err(e) => ProviderResponse::err("STATUS_FAILED", e.to_string()),
        },

        ProviderRequest::Stop => match provider.stop().await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("STOP_FAILED", e.to_string()),
        },

        ProviderRequest::Next => match provider.next().await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("NEXT_FAILED", e.to_string()),
        },

        ProviderRequest::Previous => match provider.previous().await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("PREVIOUS_FAILED", e.to_string()),
        },

        ProviderRequest::Seek { position_ms } => match provider.seek(position_ms).await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("SEEK_FAILED", e.to_string()),
        },

        ProviderRequest::SetVolume { volume } => match provider.set_volume(volume).await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("VOLUME_FAILED", e.to_string()),
        },

        ProviderRequest::GetStatus => match provider.get_status().await {
            Ok(status) => ProviderResponse::Status(status),
            Err(e) => ProviderResponse::err("STATUS_FAILED", e.to_string()),
        },

        ProviderRequest::GetQueue => match provider.get_queue().await {
            Ok(queue) => ProviderResponse::Queue(queue),
            Err(e) => ProviderResponse::err("QUEUE_FAILED", e.to_string()),
        },

        ProviderRequest::Enqueue { track } => match provider.enqueue(track).await {
            Ok(()) => ProviderResponse::Ok,
            Err(e) => ProviderResponse::err("ENQUEUE_FAILED", e.to_string()),
        },

        ProviderRequest::Action(action_req) => {
            match provider
                .custom_action(
                    &action_req.action,
                    action_req.target.as_ref(),
                    action_req.params,
                )
                .await
            {
                Ok(result) => ProviderResponse::ActionResult(result),
                Err(e) => ProviderResponse::err("ACTION_FAILED", e.to_string()),
            }
        }

        ProviderRequest::GetAuthStatus => match provider.auth_status().await {
            Ok(status) => ProviderResponse::AuthStatus(status),
            Err(e) => ProviderResponse::err("AUTH_FAILED", e.to_string()),
        },

        ProviderRequest::AuthBegin => match provider.auth_begin().await {
            Ok(status) => ProviderResponse::AuthStatus(status),
            Err(e) => ProviderResponse::err("AUTH_FAILED", e.to_string()),
        },

        ProviderRequest::AuthLogout => match provider.auth_logout().await {
            Ok(status) => ProviderResponse::AuthStatus(status),
            Err(e) => ProviderResponse::err("AUTH_FAILED", e.to_string()),
        },

        ProviderRequest::Shutdown => {
            let _ = provider.shutdown().await;
            ProviderResponse::Ok
        }
    }
}
