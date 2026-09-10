//! Malus IPC client for communicating with the malus daemon via LSP Content-Length framing.

use malus_protocol::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    client::{ClientEvent, ClientRequest, ClientResponse},
    codec::{FrameError, decode_message, read_frame, write_message},
};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::net::{
    UnixStream,
    unix::{OwnedReadHalf, OwnedWriteHalf},
};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Failed to connect to daemon at {0}: {1}")]
    ConnectionFailed(PathBuf, std::io::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol / Framing error: {0}")]
    Protocol(#[from] FrameError),
    #[error("Server returned error: [{code}] {message}")]
    ServerError { code: String, message: String },
    #[error("Unexpected response: {0:?}")]
    UnexpectedResponse(Box<ClientResponse>),
}

/// Determine the default socket path.
pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("MALUS_SOCKET") {
        return PathBuf::from(path);
    }
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("malus.sock");
    }
    PathBuf::from("/tmp/malus.sock")
}

pub struct Client {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    buf: Vec<u8>,
}

impl Client {
    /// Connect to a running Malus daemon.
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let path = socket_path.as_ref();
        let stream = UnixStream::connect(path)
            .await
            .map_err(|e| ClientError::ConnectionFailed(path.to_path_buf(), e))?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader,
            writer,
            buf: Vec::new(),
        })
    }

    /// Send a request and wait for the daemon's response.
    pub async fn send(&mut self, request: &ClientRequest) -> Result<ClientResponse, ClientError> {
        write_message(&mut self.writer, request).await?;

        match read_frame(&mut self.reader, &mut self.buf, DEFAULT_MAX_PAYLOAD_BYTES).await? {
            Some(frame) => {
                let resp: ClientResponse = decode_message(&frame)?;
                if let ClientResponse::Error { code, message } = resp {
                    Err(ClientError::ServerError { code, message })
                } else {
                    Ok(resp)
                }
            }
            None => Err(ClientError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Daemon closed connection unexpectedly",
            ))),
        }
    }

    /// Enter subscription mode and read events continuously via callback.
    pub async fn stream_events<F>(&mut self, mut on_event: F) -> Result<(), ClientError>
    where
        F: FnMut(ClientEvent),
    {
        // First request event subscription
        let resp = self.send(&ClientRequest::SubscribeEvents).await?;
        if resp != ClientResponse::Ok {
            return Err(ClientError::UnexpectedResponse(Box::new(resp)));
        }

        while let Some(frame) =
            read_frame(&mut self.reader, &mut self.buf, DEFAULT_MAX_PAYLOAD_BYTES).await?
        {
            match decode_message(&frame) {
                Ok(event) => on_event(event),
                Err(e) => eprintln!("Failed to decode event: {e}"),
            }
        }

        Ok(())
    }
}
