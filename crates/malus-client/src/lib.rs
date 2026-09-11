//! malus-client: Native Rust client library for the Malus daemon (`malusd`).
//!
//! Provides connection handling, independent RPC requests (preventing head-of-line blocking),
//! background event streaming with exponential backoff reconnection, and typed helpers.

pub mod client;
pub mod error;
pub mod socket;

pub use client::{ConnectionStatus, MalusClient};
pub use error::ClientError;
pub use socket::default_socket_path;

// Re-export protocol types for convenience
pub use malus_protocol::client::{ClientEvent, ClientRequest, ClientResponse};
pub use malus_protocol::wire::*;
