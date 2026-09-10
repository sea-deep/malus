//! Client trust boundary: RPC communication between frontends (CLI/TUI/GUI) and `malusd`.

pub mod event;
pub mod request;
pub mod response;
pub mod version;

pub use event::ClientEvent;
pub use request::ClientRequest;
pub use response::ClientResponse;
pub use version::CLIENT_PROTOCOL;
