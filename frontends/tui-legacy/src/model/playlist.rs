//! Playlists, radio stations, mixes, and library data models for Malus.

use super::track::{Album, Artist, AudioFormat, Track};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub curator: String,
    pub description: String,
    pub track_ids: Vec<String>,
}

impl Playlist {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        curator: impl Into<String>,
        description: impl Into<String>,
        track_ids: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            curator: curator.into(),
            description: description.into(),
            track_ids,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RadioStation {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub description: String,
    pub is_live: bool,
    pub current_show: String,
    pub track_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedMix {
    pub id: String,
    pub name: String,
    pub title: String,
    pub subtitle: String,
    pub track_ids: Vec<String>,
}

/// A personalised playlist recommendation from Apple Music (Heavy Rotation,
/// Your Essentials, Get Up!, Chill, New Music, etc.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PersonalMix {
    pub id: String,
    pub name: String,
    /// Short description or artist list shown beneath the name
    pub subtitle: String,
    /// Playlist kind tag from the API ("curator-playlist", "personal-mix", etc.)
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
    pub playlists: Vec<Playlist>,
    pub radio_stations: Vec<RadioStation>,
    pub curated_mixes: Vec<CuratedMix>,
    /// Personalised recommendation playlists fetched from /v1/me/recommendations
    pub personal_mixes: Vec<PersonalMix>,
    /// Actual recently played tracks fetched from /v1/me/recent/played/tracks
    pub recently_played: Vec<Track>,
}

impl Library {
    pub fn replace_tracks(&mut self, tracks: Vec<Track>) {
        use std::collections::HashMap;
        self.tracks = tracks;
        self.albums.clear();
        self.artists.clear();
        let mut albums = HashMap::<String, usize>::new();
        let mut artists = HashMap::<String, usize>::new();
        for track in &self.tracks {
            let a = *albums.entry(track.album_id.clone()).or_insert_with(|| {
                let mut album = Album::new(
                    &track.album_id,
                    &track.album,
                    track.album_artist.as_deref().unwrap_or(&track.artist),
                    0,
                    "",
                    0,
                    vec![],
                );
                album.artist_id = track.artist_id.clone();
                self.albums.push(album);
                self.albums.len() - 1
            });
            self.albums[a].track_ids.push(track.id.clone());
            self.albums[a].track_count = self.albums[a].track_ids.len();
            let i = *artists.entry(track.artist_id.clone()).or_insert_with(|| {
                self.artists.push(Artist::new(
                    &track.artist_id,
                    track.primary_artist.as_deref().unwrap_or(&track.artist),
                    "",
                    vec![],
                    vec![],
                ));
                self.artists.len() - 1
            });
            let artist = &mut self.artists[i];
            artist.top_track_ids.push(track.id.clone());
            if !artist.album_ids.contains(&track.album_id) {
                artist.album_ids.push(track.album_id.clone());
            }
        }
        let order: HashMap<_, _> = self
            .tracks
            .iter()
            .map(|t| (&t.id, (t.disc_number, t.track_number)))
            .collect();
        for album in &mut self.albums {
            album
                .track_ids
                .sort_by_key(|id| order.get(id).copied().unwrap_or_default());
        }
        self.albums.sort_by_key(|a| a.title.to_lowercase());
        self.artists.sort_by_key(|a| a.name.to_lowercase());
    }

    /// Constructs a populated sample music library to avoid hardcoding in UI components.
    pub fn mock() -> Self {
        let tracks = vec![
            Track::new(
                "trk-1",
                "Blinding Lights",
                "The Weeknd",
                "After Hours",
                200,
                1,
                AudioFormat::DolbyAtmos,
            ),
            Track::new(
                "trk-2",
                "Save Your Tears",
                "The Weeknd",
                "After Hours",
                215,
                2,
                AudioFormat::Lossless,
            ),
            Track::new(
                "trk-3",
                "As It Was",
                "Harry Styles",
                "Harry's House",
                167,
                1,
                AudioFormat::DolbyAtmos,
            ),
            Track::new(
                "trk-4",
                "Late Night Talking",
                "Harry Styles",
                "Harry's House",
                177,
                2,
                AudioFormat::Lossless,
            ),
            Track::new(
                "trk-5",
                "Starboy",
                "The Weeknd",
                "Starboy",
                230,
                1,
                AudioFormat::HiResLossless,
            ),
            Track::new(
                "trk-6",
                "Levitating",
                "Dua Lipa",
                "Future Nostalgia",
                203,
                1,
                AudioFormat::DolbyAtmos,
            ),
            Track::new(
                "trk-7",
                "Don't Start Now",
                "Dua Lipa",
                "Future Nostalgia",
                183,
                2,
                AudioFormat::Lossless,
            ),
            Track::new(
                "trk-8",
                "Midnight City",
                "M83",
                "Hurry Up, We're Dreaming",
                243,
                1,
                AudioFormat::HiResLossless,
            ),
            Track::new(
                "trk-9",
                "Get Lucky",
                "Daft Punk",
                "Random Access Memories",
                248,
                1,
                AudioFormat::HiResLossless,
            ),
            Track::new(
                "trk-10",
                "Instant Crush",
                "Daft Punk",
                "Random Access Memories",
                337,
                2,
                AudioFormat::Lossless,
            ),
            Track::new(
                "trk-11",
                "Pink + White",
                "Frank Ocean",
                "Blonde",
                184,
                1,
                AudioFormat::DolbyAtmos,
            ),
            Track::new(
                "trk-12",
                "Nights",
                "Frank Ocean",
                "Blonde",
                307,
                2,
                AudioFormat::Lossless,
            ),
        ];

        let albums = vec![
            Album::new(
                "alb-1",
                "After Hours",
                "The Weeknd",
                2020,
                "R&B / Synthpop",
                14,
                vec!["trk-1".to_string(), "trk-2".to_string()],
            ),
            Album::new(
                "alb-2",
                "Harry's House",
                "Harry Styles",
                2022,
                "Pop / Rock",
                13,
                vec!["trk-3".to_string(), "trk-4".to_string()],
            ),
            Album::new(
                "alb-3",
                "Future Nostalgia",
                "Dua Lipa",
                2020,
                "Nu-Disco / Pop",
                11,
                vec!["trk-6".to_string(), "trk-7".to_string()],
            ),
            Album::new(
                "alb-4",
                "Random Access Memories",
                "Daft Punk",
                2013,
                "Disco / Funk",
                13,
                vec!["trk-9".to_string(), "trk-10".to_string()],
            ),
            Album::new(
                "alb-5",
                "Blonde",
                "Frank Ocean",
                2016,
                "R&B / Soul",
                17,
                vec!["trk-11".to_string(), "trk-12".to_string()],
            ),
        ];

        let artists = vec![
            Artist::new(
                "art-1",
                "The Weeknd",
                "R&B / Pop",
                vec![
                    "trk-1".to_string(),
                    "trk-2".to_string(),
                    "trk-5".to_string(),
                ],
                vec!["alb-1".to_string()],
            ),
            Artist::new(
                "art-2",
                "Frank Ocean",
                "R&B / Soul",
                vec!["trk-11".to_string(), "trk-12".to_string()],
                vec!["alb-5".to_string()],
            ),
            Artist::new(
                "art-3",
                "Daft Punk",
                "Electronic",
                vec!["trk-9".to_string(), "trk-10".to_string()],
                vec!["alb-4".to_string()],
            ),
            Artist::new(
                "art-4",
                "Dua Lipa",
                "Pop",
                vec!["trk-6".to_string(), "trk-7".to_string()],
                vec!["alb-3".to_string()],
            ),
        ];

        let playlists = vec![
            Playlist {
                id: "pl-1".to_string(),
                name: "Today's Hits".to_string(),
                curator: "Apple Music Pop".to_string(),
                description: "The biggest songs right now across all genres.".to_string(),
                track_ids: vec![
                    "trk-1".to_string(),
                    "trk-3".to_string(),
                    "trk-6".to_string(),
                    "trk-11".to_string(),
                ],
            },
            Playlist {
                id: "pl-2".to_string(),
                name: "Spatial Audio Hits".to_string(),
                curator: "Apple Music Spatial".to_string(),
                description: "Hear sound all around you with Dolby Atmos.".to_string(),
                track_ids: vec![
                    "trk-1".to_string(),
                    "trk-3".to_string(),
                    "trk-6".to_string(),
                ],
            },
            Playlist {
                id: "pl-3".to_string(),
                name: "Night Grooves".to_string(),
                curator: "Apple Music Electronic".to_string(),
                description: "Late night synth and electronic grooves.".to_string(),
                track_ids: vec![
                    "trk-8".to_string(),
                    "trk-9".to_string(),
                    "trk-10".to_string(),
                    "trk-12".to_string(),
                ],
            },
            Playlist {
                id: "pl-4".to_string(),
                name: "Frank Ocean Essentials".to_string(),
                curator: "Apple Music R&B".to_string(),
                description: "The visionary singer-songwriter's essential work.".to_string(),
                track_ids: vec!["trk-11".to_string(), "trk-12".to_string()],
            },
        ];

        let radio_stations = vec![
            RadioStation {
                id: "rad-1".to_string(),
                name: "Apple Music 1".to_string(),
                tag: "LIVE BROADCAST".to_string(),
                description: "The new music that matters today, broadcast live globally from LA, NY, and London.".to_string(),
                is_live: true,
                current_show: "The Zane Lowe Show".to_string(),
                track_ids: vec!["trk-1".to_string(), "trk-3".to_string(), "trk-6".to_string()],
            },
            RadioStation {
                id: "rad-2".to_string(),
                name: "Apple Music Hits".to_string(),
                tag: "LIVE BROADCAST".to_string(),
                description: "The songs you know and love from the '80s, '90s, and 2000s.".to_string(),
                is_live: true,
                current_show: "Daily Throwback".to_string(),
                track_ids: vec!["trk-8".to_string(), "trk-9".to_string()],
            },
            RadioStation {
                id: "rad-3".to_string(),
                name: "Discovery Station".to_string(),
                tag: "FOR YOU".to_string(),
                description: "Personalized stream of music tailored to what you love.".to_string(),
                is_live: false,
                current_show: "Personalized Stream".to_string(),
                track_ids: vec!["trk-10".to_string(), "trk-11".to_string(), "trk-12".to_string()],
            },
            RadioStation {
                id: "rad-4".to_string(),
                name: "Frank Ocean Station".to_string(),
                tag: "ARTIST RADIO".to_string(),
                description: "Songs by Frank Ocean and artists in his sonic orbit.".to_string(),
                is_live: false,
                current_show: "Artist Wave".to_string(),
                track_ids: vec!["trk-11".to_string(), "trk-12".to_string()],
            },
        ];

        let curated_mixes = vec![
            CuratedMix {
                id: "mix-1".to_string(),
                name: "Chill Mix".to_string(),
                title: "Chill Mix".to_string(),
                subtitle: "Updated Friday".to_string(),
                track_ids: vec![
                    "trk-11".to_string(),
                    "trk-12".to_string(),
                    "trk-2".to_string(),
                ],
            },
            CuratedMix {
                id: "mix-2".to_string(),
                name: "Favorites Mix".to_string(),
                title: "Favorites Mix".to_string(),
                subtitle: "Updated Tuesday".to_string(),
                track_ids: vec![
                    "trk-1".to_string(),
                    "trk-6".to_string(),
                    "trk-8".to_string(),
                ],
            },
            CuratedMix {
                id: "mix-3".to_string(),
                name: "Get Up! Mix".to_string(),
                title: "Get Up! Mix".to_string(),
                subtitle: "Updated Monday".to_string(),
                track_ids: vec![
                    "trk-9".to_string(),
                    "trk-3".to_string(),
                    "trk-7".to_string(),
                ],
            },
        ];

        Self {
            tracks,
            albums,
            artists,
            playlists,
            radio_stations,
            curated_mixes,
            personal_mixes: vec![],
            recently_played: vec![],
        }
    }

    pub fn find_track(&self, id: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn find_album(&self, id: &str) -> Option<&Album> {
        self.albums.iter().find(|a| a.id == id)
    }

    pub fn find_artist(&self, id_or_name: &str) -> Option<&Artist> {
        self.artists
            .iter()
            .find(|a| a.id == id_or_name || a.name.eq_ignore_ascii_case(id_or_name))
    }

    pub fn find_playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn find_station(&self, id: &str) -> Option<&RadioStation> {
        self.radio_stations.iter().find(|s| s.id == id)
    }

    pub fn find_mix(&self, id: &str) -> Option<&CuratedMix> {
        self.curated_mixes.iter().find(|m| m.id == id)
    }
}
