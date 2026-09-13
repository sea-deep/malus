pub mod api;
pub mod auth;
pub mod error;
pub mod provider;
pub mod web;

pub use api::{
    AppleApiError, AppleCredentials, OfficialAppleMusicApi, ProfileTokenProvider,
    StaticTokenProvider, TokenProvider,
};
pub use auth::AuthState;
pub use error::AppleError;
pub use provider::AppleProvider;
pub use web::{
    AppleWebSession, ProductionAppleWebSession, parse_apple_album, parse_apple_artist,
    parse_apple_artwork, parse_apple_playlist, parse_apple_track,
};
