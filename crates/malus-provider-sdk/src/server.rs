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
    provider::{PROVIDER_PROTOCOL, ProviderRequest, ProviderResponse},
};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

/// Serve a provider over an arbitrary async reader and writer.
pub async fn serve_io<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    provider: Arc<dyn Provider>,
    mut reader: R,
    mut writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = Vec::new();

    while let Some(frame) = read_frame(&mut reader, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES).await? {
        let req: ProviderRequest = match decode_message(&frame) {
            Ok(r) => r,
            Err(e) => {
                let err_res = ProviderResponse::err("INVALID_REQUEST", e.to_string());
                write_message(&mut writer, &err_res).await?;
                continue;
            }
        };

        let is_shutdown = matches!(req, ProviderRequest::Shutdown);
        let resp = handle_request(&*provider, req).await;
        write_message(&mut writer, &resp).await?;

        if is_shutdown {
            break;
        }
    }

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

        ProviderRequest::Search { query, limit } => match provider.search(&query, limit).await {
            Ok(tracks) => ProviderResponse::SearchResults { tracks },
            Err(e) => ProviderResponse::err("SEARCH_FAILED", e.to_string()),
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

        ProviderRequest::Shutdown => {
            let _ = provider.shutdown().await;
            ProviderResponse::Ok
        }
    }
}
