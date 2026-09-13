//! Official Apple Music HTTP API client and data models.

pub mod credentials;
pub mod credits;
pub mod error;
pub mod lyrics;
pub mod official;
pub mod parse;

pub use credentials::{AppleCredentials, ProfileTokenProvider, StaticTokenProvider, TokenProvider};
pub use credits::parse_apple_credits;
pub use error::AppleApiError;
pub use lyrics::parse_ttml_lyrics;
pub use official::{DEFAULT_APPLE_API_BASE, OfficialAppleMusicApi};
pub use parse::{
    parse_apple_album, parse_apple_artist, parse_apple_artwork, parse_apple_playlist,
    parse_apple_track,
};
