use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFormat {
    Standard,
    Lossless,
    HiResLossless,
    DolbyAtmos,
}

impl AudioFormat {
    pub const fn badge(&self) -> &'static str {
        match self {
            Self::Standard => "AAC",
            Self::Lossless => "LOSSLESS",
            Self::HiResLossless => "HI-RES LOSSLESS",
            Self::DolbyAtmos => "SPATIAL AUDIO",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    #[serde(default)]
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub album_artist: Option<String>,
    #[serde(default)]
    pub primary_artist: Option<String>,
    #[serde(default = "default_disc")]
    pub disc_number: u32,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub album: String,
    pub album_id: String,
    pub duration_secs: u64,
    pub is_favorite: bool,
    pub is_loved: bool,
    pub format: AudioFormat,
    pub track_number: u32,
}

impl Track {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
        duration_secs: u64,
        track_number: u32,
        format: AudioFormat,
    ) -> Self {
        let artist_str = artist.into();
        let album_str = album.into();
        let artist_id = format!("art-{}", artist_str.to_lowercase().replace(' ', "-"));
        let album_id = format!("album:{artist_str}:{album_str}");
        Self {
            id: id.into(),
            catalog_id: None,
            artwork_url: None,
            album_artist: None,
            primary_artist: None,
            disc_number: 1,
            title: title.into(),
            artist: artist_str,
            artist_id,
            album: album_str,
            album_id,
            duration_secs,
            is_favorite: false,
            is_loved: false,
            format,
            track_number,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_secs * 1000
    }

    pub fn playback_id(&self) -> &str {
        self.catalog_id.as_deref().unwrap_or(&self.id)
    }

    pub fn formatted_duration(&self) -> String {
        let mins = self.duration_secs / 60;
        let secs = self.duration_secs % 60;
        format!("{mins}:{secs:02}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub year: u32,
    pub genre: String,
    pub track_count: usize,
    pub track_ids: Vec<String>,
}

impl Album {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
        year: u32,
        genre: impl Into<String>,
        track_count: usize,
        track_ids: Vec<String>,
    ) -> Self {
        let artist_str = artist.into();
        let artist_id = format!("art-{}", artist_str.to_lowercase().replace(' ', "-"));
        Self {
            id: id.into(),
            title: title.into(),
            artist: artist_str,
            artist_id,
            year,
            genre: genre.into(),
            track_count,
            track_ids,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub genre: String,
    pub top_track_ids: Vec<String>,
    pub album_ids: Vec<String>,
}

impl Artist {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        genre: impl Into<String>,
        top_track_ids: Vec<String>,
        album_ids: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            genre: genre.into(),
            top_track_ids,
            album_ids,
        }
    }
}

fn default_disc() -> u32 {
    1
}
