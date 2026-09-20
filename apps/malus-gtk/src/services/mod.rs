pub mod artwork;
pub mod recent_searches;

pub use artwork::{
    ArtworkService, DecodedImage, THUMB_LARGE, THUMB_MEDIUM, THUMB_SMALL, artwork_texture,
    bind_artwork, generate_backdrop_texture,
};
pub use recent_searches::{
    RecentSearchItem, add_recent_search, clear_recent_searches, load_recent_searches,
};
