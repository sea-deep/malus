//! Apple Music provider library.

pub mod auth;
pub mod error;
pub mod provider;
pub mod web;

pub use auth::AuthState;
pub use error::AppleError;
pub use provider::AppleProvider;
pub use web::AppleWebSession;
