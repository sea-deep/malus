//! Pure domain models for audio media entities and references.

pub mod album;
pub mod artist;
pub mod artwork;
pub mod playlist;
pub mod track;

pub use album::{Album, AlbumRef};
pub use artist::{Artist, ArtistRef};
pub use artwork::Artwork;
pub use playlist::Playlist;
pub use track::Track;
