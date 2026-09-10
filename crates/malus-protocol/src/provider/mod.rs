//! Provider trust boundary: RPC communication between `malusd` and provider processes.

pub mod event;
pub mod request;
pub mod response;
pub mod version;

pub use event::ProviderEvent;
pub use request::ProviderRequest;
pub use response::ProviderResponse;
pub use version::PROVIDER_PROTOCOL;
