//! Pure domain model for artwork metadata.

/// Pure metadata reference to an artwork asset.
///
/// Contains no image data, HTTP clients, or caching semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Artwork {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl Artwork {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            width: None,
            height: None,
        }
    }

    pub fn with_dimensions(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}
