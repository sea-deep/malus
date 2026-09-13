//! Behavioral Mock Audio Provider implementation.
//!
//! Conforms to Malus behavioral provider architecture:
//! - Strictly namespaced IDs (`mock:track:1`, `mock:album:1`, `mock:artist:1`)
//! - Monotonic playback clock (`std::time::Instant`) advancing during playback
//! - Pausing freezes position; seeking updates clock base
//! - Authoritative internal playback queue
//! - Declares capabilities (`search`, `catalog.track`, `catalog.album`, `catalog.artist`,
//!   `catalog.playlist`, `library.tracks`, `library.albums`, `library.playlists`,
//!   `playback`, `playback.seek`, `queue.read`, `queue.edit`, `mock.repost`)
//! - Exposes custom action `mock.repost` via generic RPC
//! - Excludes `lyrics` capability

use async_trait::async_trait;
use malus_provider_sdk::{
    ActionRoleWire, ActionStateWire, AlbumRefWire, AlbumWire, ArtistRefWire, ArtistWire,
    CatalogItemWire, LibraryKindWire, LibraryPageWire, MediaIdWire, PageWire, PlaybackStateWire,
    PlayerStatusWire, PlaylistWire, ProviderEntityRefWire, ProviderSurfaceManifestWire, QueueWire,
    RepeatModeWire, SearchKindWire, SearchResultsWire, SurfaceActionResultWire, SurfaceActionWire,
    SurfaceBadgeWire, SurfaceContinuationWire, SurfaceCursorWire, SurfaceHeaderWire,
    SurfaceItemWire, SurfaceNavEntryWire, SurfaceNavGroupWire, SurfaceRefreshWire,
    SurfaceSectionWire, SurfaceWire, TrackWire, capability, error::ProviderError, traits::Provider,
};
use std::{sync::Arc, time::Instant};
use tokio::sync::Mutex;

struct MockState {
    tracks: Vec<TrackWire>,
    albums: Vec<AlbumWire>,
    artists: Vec<ArtistWire>,
    playlists: Vec<PlaylistWire>,
    queue: Vec<TrackWire>,
    current_index: Option<usize>,
    playback_state: PlaybackStateWire,
    position_base_ms: u64,
    last_started_at: Option<Instant>,
    volume: u8,
}

impl MockState {
    fn new() -> Self {
        let tracks = vec![
            TrackWire {
                id: "mock:track:1".into(),
                title: "Mock Track 1".into(),
                artists: vec![ArtistRefWire::named("Mock Artist")],
                album: Some(AlbumRefWire::new(Some("mock:album:1"), "Mock Album A")),
                duration_ms: Some(180_000),
                track_number: Some(1),
                disc_number: Some(1),
                explicit: Some(false),
                artwork: None,
                uri: Some("mock://tracks/1".into()),
            },
            TrackWire {
                id: "mock:track:2".into(),
                title: "Mock Track 2".into(),
                artists: vec![ArtistRefWire::named("Mock Artist")],
                album: Some(AlbumRefWire::new(Some("mock:album:1"), "Mock Album A")),
                duration_ms: Some(210_000),
                track_number: Some(2),
                disc_number: Some(1),
                explicit: Some(false),
                artwork: None,
                uri: Some("mock://tracks/2".into()),
            },
            TrackWire {
                id: "mock:track:3".into(),
                title: "Acoustic Sunset".into(),
                artists: vec![ArtistRefWire::named("Solaris")],
                album: Some(AlbumRefWire::new(Some("mock:album:2"), "Mock Album B")),
                duration_ms: Some(245_000),
                track_number: Some(1),
                disc_number: Some(1),
                explicit: Some(false),
                artwork: None,
                uri: Some("mock://tracks/3".into()),
            },
        ];

        let albums = vec![
            AlbumWire {
                id: "mock:album:1".into(),
                title: "Mock Album A".into(),
                artists: vec![ArtistRefWire::named("Mock Artist")],
                release_date: Some("2021-05-10".into()),
                track_count: Some(2),
                artwork: None,
            },
            AlbumWire {
                id: "mock:album:2".into(),
                title: "Mock Album B".into(),
                artists: vec![ArtistRefWire::named("Solaris")],
                release_date: Some("2023-11-20".into()),
                track_count: Some(1),
                artwork: None,
            },
        ];

        let artists = vec![
            ArtistWire {
                id: "mock:artist:1".into(),
                name: "Mock Artist".into(),
                artwork: None,
            },
            ArtistWire {
                id: "mock:artist:2".into(),
                name: "Solaris".into(),
                artwork: None,
            },
        ];

        let playlists = vec![PlaylistWire {
            id: "mock:playlist:1".into(),
            title: "Mock Chill Hits".into(),
            curator: Some("Mock Curator".into()),
            description: Some("Relaxing mock tracks".into()),
            track_count: Some(3),
            artwork: None,
        }];

        Self {
            queue: tracks.clone(),
            current_index: Some(0),
            tracks,
            albums,
            artists,
            playlists,
            playback_state: PlaybackStateWire::Stopped,
            position_base_ms: 0,
            last_started_at: None,
            volume: 100,
        }
    }

    fn current_track(&self) -> Option<&TrackWire> {
        self.current_index.and_then(|i| self.queue.get(i))
    }

    fn current_position_ms(&self) -> u64 {
        let duration = self
            .current_track()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0);
        let pos = match (self.playback_state, self.last_started_at) {
            (PlaybackStateWire::Playing, Some(start)) => {
                self.position_base_ms + (start.elapsed().as_millis() as u64)
            }
            _ => self.position_base_ms,
        };
        if duration > 0 { pos.min(duration) } else { pos }
    }
}

pub struct MockProvider {
    state: Arc<Mutex<MockState>>,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::new())),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn name(&self) -> &str {
        "Mock Audio Provider"
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            capability::SEARCH.into(),
            capability::CATALOG_TRACK.into(),
            capability::CATALOG_ALBUM.into(),
            capability::CATALOG_ARTIST.into(),
            capability::CATALOG_PLAYLIST.into(),
            capability::LIBRARY_TRACKS.into(),
            capability::LIBRARY_ALBUMS.into(),
            capability::LIBRARY_PLAYLISTS.into(),
            capability::PLAYBACK.into(),
            capability::PLAYBACK_SEEK.into(),
            capability::QUEUE_READ.into(),
            capability::QUEUE_EDIT.into(),
            capability::SURFACES.into(),
            "mock.repost".into(),
        ]
    }

    async fn search(
        &self,
        query: &str,
        kinds: &[SearchKindWire],
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResultsWire, ProviderError> {
        let _ = cursor;
        let state = self.state.lock().await;
        let q = query.to_lowercase();

        let filter_tracks = kinds.is_empty() || kinds.contains(&SearchKindWire::Track);
        let filter_albums = kinds.is_empty() || kinds.contains(&SearchKindWire::Album);
        let filter_artists = kinds.is_empty() || kinds.contains(&SearchKindWire::Artist);
        let filter_playlists = kinds.is_empty() || kinds.contains(&SearchKindWire::Playlist);

        let tracks = if filter_tracks {
            let matched: Vec<_> = state
                .tracks
                .iter()
                .filter(|t| {
                    t.title.to_lowercase().contains(&q)
                        || t.artist_display().to_lowercase().contains(&q)
                        || t.id.to_lowercase().contains(&q)
                })
                .take(limit)
                .cloned()
                .collect();
            Some(PageWire::new(matched, None))
        } else {
            None
        };

        let albums = if filter_albums {
            let matched: Vec<_> = state
                .albums
                .iter()
                .filter(|a| {
                    a.title.to_lowercase().contains(&q)
                        || a.artist_display().to_lowercase().contains(&q)
                        || a.id.to_lowercase().contains(&q)
                })
                .take(limit)
                .cloned()
                .collect();
            Some(PageWire::new(matched, None))
        } else {
            None
        };

        let artists = if filter_artists {
            let matched: Vec<_> = state
                .artists
                .iter()
                .filter(|a| a.name.to_lowercase().contains(&q) || a.id.to_lowercase().contains(&q))
                .take(limit)
                .cloned()
                .collect();
            Some(PageWire::new(matched, None))
        } else {
            None
        };

        let playlists = if filter_playlists {
            let matched: Vec<_> = state
                .playlists
                .iter()
                .filter(|p| p.title.to_lowercase().contains(&q) || p.id.to_lowercase().contains(&q))
                .take(limit)
                .cloned()
                .collect();
            Some(PageWire::new(matched, None))
        } else {
            None
        };

        Ok(SearchResultsWire {
            tracks,
            albums,
            artists,
            playlists,
        })
    }

    async fn get_catalog_item(&self, media_id: &str) -> Result<CatalogItemWire, ProviderError> {
        let state = self.state.lock().await;
        if let Some(track) = state.tracks.iter().find(|t| t.id == media_id) {
            return Ok(CatalogItemWire::Track(track.clone()));
        }
        if let Some(album) = state.albums.iter().find(|a| a.id == media_id) {
            return Ok(CatalogItemWire::Album(album.clone()));
        }
        if let Some(artist) = state.artists.iter().find(|a| a.id == media_id) {
            return Ok(CatalogItemWire::Artist(artist.clone()));
        }
        if let Some(playlist) = state.playlists.iter().find(|p| p.id == media_id) {
            return Ok(CatalogItemWire::Playlist(playlist.clone()));
        }
        Err(ProviderError::NotFound(media_id.to_string()))
    }

    async fn get_collection_items(
        &self,
        media_id: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<PageWire<TrackWire>, ProviderError> {
        let _ = cursor;
        let state = self.state.lock().await;
        if media_id.starts_with("mock:album:") {
            let album_tracks: Vec<_> = state
                .tracks
                .iter()
                .filter(|t| t.album.as_ref().and_then(|a| a.id.as_deref()) == Some(media_id))
                .take(limit)
                .cloned()
                .collect();
            Ok(PageWire::new(album_tracks, None))
        } else if media_id.starts_with("mock:playlist:") {
            let playlist_tracks: Vec<_> = state.tracks.iter().take(limit).cloned().collect();
            Ok(PageWire::new(playlist_tracks, None))
        } else {
            Err(ProviderError::NotFound(media_id.to_string()))
        }
    }

    async fn get_library(
        &self,
        kind: LibraryKindWire,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<LibraryPageWire, ProviderError> {
        let _ = cursor;
        let state = self.state.lock().await;
        match kind {
            LibraryKindWire::Tracks => {
                let items: Vec<_> = state.tracks.iter().take(limit).cloned().collect();
                Ok(LibraryPageWire::Tracks(PageWire::new(items, None)))
            }
            LibraryKindWire::Albums => {
                let items: Vec<_> = state.albums.iter().take(limit).cloned().collect();
                Ok(LibraryPageWire::Albums(PageWire::new(items, None)))
            }
            LibraryKindWire::Playlists => {
                let items: Vec<_> = state.playlists.iter().take(limit).cloned().collect();
                Ok(LibraryPageWire::Playlists(PageWire::new(items, None)))
            }
        }
    }

    async fn play(&self, media_id: &str) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let found_index = state.queue.iter().position(|t| t.id == media_id);

        let idx = if let Some(i) = found_index {
            i
        } else if let Some(track) = state.tracks.iter().find(|t| t.id == media_id).cloned() {
            state.queue.push(track);
            state.queue.len() - 1
        } else {
            return Err(ProviderError::NotFound(media_id.to_string()));
        };

        state.current_index = Some(idx);
        state.position_base_ms = 0;
        state.last_started_at = Some(Instant::now());
        state.playback_state = PlaybackStateWire::Playing;
        Ok(())
    }

    async fn pause(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let current_pos = state.current_position_ms();
        state.position_base_ms = current_pos;
        state.last_started_at = None;
        state.playback_state = PlaybackStateWire::Paused;
        Ok(())
    }

    async fn resume(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.playback_state == PlaybackStateWire::Playing {
            return Ok(());
        }
        state.last_started_at = Some(Instant::now());
        state.playback_state = PlaybackStateWire::Playing;
        Ok(())
    }

    async fn stop(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        state.playback_state = PlaybackStateWire::Stopped;
        state.position_base_ms = 0;
        state.last_started_at = None;
        Ok(())
    }

    async fn next(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.queue.is_empty() {
            return Ok(());
        }
        let next_idx = match state.current_index {
            Some(i) => (i + 1).min(state.queue.len() - 1),
            None => 0,
        };
        state.current_index = Some(next_idx);
        state.position_base_ms = 0;
        state.last_started_at = if state.playback_state == PlaybackStateWire::Playing {
            Some(Instant::now())
        } else {
            None
        };
        Ok(())
    }

    async fn previous(&self) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.queue.is_empty() {
            return Ok(());
        }
        let prev_idx = match state.current_index {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        state.current_index = Some(prev_idx);
        state.position_base_ms = 0;
        state.last_started_at = if state.playback_state == PlaybackStateWire::Playing {
            Some(Instant::now())
        } else {
            None
        };
        Ok(())
    }

    async fn seek(&self, position_ms: u64) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        let duration = state
            .current_track()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0);
        let clamped = if duration > 0 {
            position_ms.min(duration)
        } else {
            position_ms
        };
        state.position_base_ms = clamped;
        if state.playback_state == PlaybackStateWire::Playing {
            state.last_started_at = Some(Instant::now());
        }
        Ok(())
    }

    async fn set_volume(&self, volume: u8) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        state.volume = volume.min(100);
        Ok(())
    }

    async fn get_status(&self) -> Result<PlayerStatusWire, ProviderError> {
        let state = self.state.lock().await;
        let current_track = state.current_track().cloned();
        let position_ms = state.current_position_ms();
        let duration_ms = current_track
            .as_ref()
            .and_then(|t| t.duration_ms)
            .unwrap_or(0);

        Ok(PlayerStatusWire {
            state: state.playback_state,
            current_track,
            position_ms,
            duration_ms,
            volume: state.volume,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        })
    }

    async fn get_queue(&self) -> Result<QueueWire, ProviderError> {
        let state = self.state.lock().await;
        Ok(QueueWire {
            items: state.queue.clone(),
            current_index: state.current_index,
        })
    }

    async fn enqueue(&self, track: TrackWire) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        state.queue.push(track);
        if state.current_index.is_none() {
            state.current_index = Some(0);
        }
        Ok(())
    }

    async fn custom_action(
        &self,
        action: &str,
        target: Option<&MediaIdWire>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let _ = params;
        if action == "mock.repost" {
            let target_str = target.map(|t| t.as_str());
            Ok(serde_json::json!({
                "reposted": true,
                "track_id": target_str,
                "provider": "mock",
            }))
        } else {
            Err(ProviderError::NotSupported(format!(
                "Action '{action}' is not supported"
            )))
        }
    }

    async fn get_surface_manifest(&self) -> Result<ProviderSurfaceManifestWire, ProviderError> {
        Ok(ProviderSurfaceManifestWire {
            provider_id: "mock".to_string(),
            default_surface_id: "home".to_string(),
            groups: vec![
                SurfaceNavGroupWire::new(
                    "discover",
                    Some("Discover".to_string()),
                    vec![
                        SurfaceNavEntryWire::with_icon("home", "Home", "house"),
                        SurfaceNavEntryWire::with_icon("explore", "Explore", "compass"),
                    ],
                ),
                SurfaceNavGroupWire::new(
                    "library",
                    Some("Library".to_string()),
                    vec![
                        SurfaceNavEntryWire::with_icon("library/albums", "Albums", "record-vinyl"),
                        SurfaceNavEntryWire::with_icon("library/tracks", "Tracks", "music-note"),
                    ],
                ),
            ],
        })
    }

    async fn get_surface(&self, surface_id: &str) -> Result<SurfaceWire, ProviderError> {
        let state = self.state.lock().await;
        match surface_id {
            "home" => {
                let mut surface = SurfaceWire::new("home", "Home");
                surface.subtitle = Some("Welcome to Mock Music".to_string());

                // Section 1: Recently Played (Shelf)
                let recent_items: Vec<SurfaceItemWire> = state
                    .albums
                    .iter()
                    .map(|album| {
                        let mut item = SurfaceItemWire::new(&album.id, &album.title);
                        item.subtitle = Some(album.artist_display());
                        item.entity = Some(ProviderEntityRefWire::new("mock", &album.id, "album"));
                        item.open_surface_id = Some(format!("album:{}", album.id));
                        item.actions = vec![SurfaceActionWire::new(
                            format!("mock:act:play:{}", album.id),
                            "Play",
                            ActionRoleWire::Primary,
                        )];
                        item.presentation_hint = Some("card".to_string());
                        item
                    })
                    .collect();

                let recent_sec = SurfaceSectionWire::new(
                    "recently-played",
                    Some("Recently Played".to_string()),
                    recent_items,
                )
                .with_hint("shelf");
                surface.sections.push(recent_sec);

                // Section 2: Discover Weekly (Shelf with continuation & action)
                let mut disc_items = Vec::new();
                if let Some(t3) = state.tracks.iter().find(|t| t.id == "mock:track:3") {
                    let mut item = SurfaceItemWire::new(&t3.id, &t3.title);
                    item.subtitle = Some(t3.artist_display());
                    item.entity = Some(ProviderEntityRefWire::new("mock", &t3.id, "song"));
                    item.badges = vec![SurfaceBadgeWire::new("Staff Pick")];
                    item.actions = vec![
                        SurfaceActionWire::new(
                            format!("mock:act:play:{}", t3.id),
                            "Play",
                            ActionRoleWire::Primary,
                        ),
                        SurfaceActionWire::toggle(
                            format!("mock:act:fav:{}", t3.id),
                            "Favorite",
                            ActionStateWire::Active,
                        ),
                    ];
                    disc_items.push(item);
                }

                let mut disc_sec = SurfaceSectionWire::new(
                    "discover-weekly",
                    Some("Discover Weekly".to_string()),
                    disc_items,
                )
                .with_hint("shelf");
                disc_sec.continuation = Some(SurfaceCursorWire::section(
                    "discover-weekly",
                    "cursor:discover:page2",
                ));
                surface.sections.push(disc_sec);

                Ok(surface)
            }

            "library/albums" => {
                let mut surface = SurfaceWire::new("library/albums", "Albums");
                let items: Vec<SurfaceItemWire> = state
                    .albums
                    .iter()
                    .map(|album| {
                        let mut item = SurfaceItemWire::new(&album.id, &album.title);
                        item.subtitle = Some(album.artist_display());
                        item.entity = Some(ProviderEntityRefWire::new("mock", &album.id, "album"));
                        item.open_surface_id = Some(format!("album:{}", album.id));
                        item.actions = vec![SurfaceActionWire::new(
                            format!("mock:act:play:{}", album.id),
                            "Play",
                            ActionRoleWire::Primary,
                        )];
                        item
                    })
                    .collect();

                surface
                    .sections
                    .push(SurfaceSectionWire::new("all-albums", None, items).with_hint("grid"));
                Ok(surface)
            }

            "library/tracks" => {
                let mut surface = SurfaceWire::new("library/tracks", "Tracks");
                let items: Vec<SurfaceItemWire> = state
                    .tracks
                    .iter()
                    .map(|track| {
                        let mut item = SurfaceItemWire::new(&track.id, &track.title);
                        item.subtitle = Some(track.artist_display());
                        item.entity = Some(ProviderEntityRefWire::new("mock", &track.id, "song"));
                        if let Some(dur) = track.duration_ms {
                            let mins = dur / 60_000;
                            let secs = (dur % 60_000) / 1000;
                            item.metadata.push(format!("{}:{:02}", mins, secs));
                        }
                        item.actions = vec![
                            SurfaceActionWire::new(
                                format!("mock:act:play:{}", track.id),
                                "Play",
                                ActionRoleWire::Primary,
                            ),
                            SurfaceActionWire::toggle(
                                format!("mock:act:fav:{}", track.id),
                                "Favorite",
                                ActionStateWire::Inactive,
                            ),
                        ];
                        item
                    })
                    .collect();

                surface.sections.push(
                    SurfaceSectionWire::new("all-tracks", None, items).with_hint("track-list"),
                );
                Ok(surface)
            }

            s if s.starts_with("album:") => {
                let album_id = &s["album:".len()..];
                let album = state
                    .albums
                    .iter()
                    .find(|a| a.id == album_id)
                    .ok_or_else(|| ProviderError::NotFound(s.to_string()))?;

                let mut surface = SurfaceWire::new(s, &album.title);
                let mut header = SurfaceHeaderWire::new(&album.title);
                header.subtitle = Some(album.artist_display());
                header.badges = vec![SurfaceBadgeWire::new("Lossless Available")];
                header.actions = vec![
                    SurfaceActionWire::new(
                        format!("mock:act:play:{}", album.id),
                        "Play",
                        ActionRoleWire::Primary,
                    ),
                    SurfaceActionWire::toggle(
                        format!("mock:act:fav:{}", album.id),
                        "Favorite",
                        ActionStateWire::Active,
                    ),
                ];
                surface.header = Some(header);

                let album_tracks: Vec<SurfaceItemWire> = state
                    .tracks
                    .iter()
                    .filter(|t| t.album.as_ref().and_then(|a| a.id.as_deref()) == Some(album_id))
                    .map(|t| {
                        let mut item = SurfaceItemWire::new(&t.id, &t.title);
                        item.subtitle = Some(t.artist_display());
                        item.entity = Some(ProviderEntityRefWire::new("mock", &t.id, "song"));
                        item.actions = vec![SurfaceActionWire::new(
                            format!("mock:act:play:{}", t.id),
                            "Play",
                            ActionRoleWire::Primary,
                        )];
                        item
                    })
                    .collect();

                surface.sections.push(
                    SurfaceSectionWire::new("tracks", None, album_tracks).with_hint("track-list"),
                );
                Ok(surface)
            }

            other => Err(ProviderError::NotFound(format!(
                "Surface '{other}' not found"
            ))),
        }
    }

    async fn continue_surface(
        &self,
        surface_id: &str,
        cursor: &SurfaceCursorWire,
    ) -> Result<SurfaceContinuationWire, ProviderError> {
        let state = self.state.lock().await;
        if surface_id == "home" && cursor.token == "cursor:discover:page2" {
            let more_items: Vec<SurfaceItemWire> = state
                .tracks
                .iter()
                .filter(|t| t.id != "mock:track:3")
                .map(|t| {
                    let mut item = SurfaceItemWire::new(&t.id, &t.title);
                    item.subtitle = Some(t.artist_display());
                    item.entity = Some(ProviderEntityRefWire::new("mock", &t.id, "song"));
                    item.actions = vec![SurfaceActionWire::new(
                        format!("mock:act:play:{}", t.id),
                        "Play",
                        ActionRoleWire::Primary,
                    )];
                    item
                })
                .collect();

            Ok(SurfaceContinuationWire::Section {
                section_id: "discover-weekly".to_string(),
                items: more_items,
                continuation: None,
            })
        } else {
            Err(ProviderError::NotFound(format!(
                "Continuation not found for cursor '{}'",
                cursor.token
            )))
        }
    }

    async fn invoke_surface_action(
        &self,
        invocation_token: &str,
    ) -> Result<SurfaceActionResultWire, ProviderError> {
        if let Some(target) = invocation_token.strip_prefix("mock:act:play:") {
            self.play(target).await?;
            Ok(SurfaceActionResultWire::success())
        } else if let Some(_target) = invocation_token.strip_prefix("mock:act:fav:") {
            Ok(SurfaceActionResultWire::success().with_refresh(SurfaceRefreshWire::CurrentSurface))
        } else {
            Ok(SurfaceActionResultWire::failed(format!(
                "Unsupported action: {invocation_token}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use malus_provider_sdk::ActionStatusWire;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_mock_provider_monotonic_clock_advancement() {
        let p = MockProvider::new();
        p.play("mock:track:1").await.unwrap();

        let s1 = p.get_status().await.unwrap();
        assert_eq!(s1.state, PlaybackStateWire::Playing);
        let pos1 = s1.position_ms;

        // Sleep 60ms
        sleep(Duration::from_millis(60)).await;

        let s2 = p.get_status().await.unwrap();
        let pos2 = s2.position_ms;
        assert!(
            pos2 >= pos1 + 50,
            "Playback clock should advance while playing"
        );

        // Pause
        p.pause().await.unwrap();
        let s3 = p.get_status().await.unwrap();
        assert_eq!(s3.state, PlaybackStateWire::Paused);
        let pos3 = s3.position_ms;

        // Sleep while paused
        sleep(Duration::from_millis(50)).await;
        let s4 = p.get_status().await.unwrap();
        assert_eq!(
            s4.position_ms, pos3,
            "Position must not advance while paused"
        );
    }

    #[tokio::test]
    async fn test_mock_provider_seek() {
        let p = MockProvider::new();
        p.play("mock:track:1").await.unwrap();
        p.seek(50_000).await.unwrap();

        let s = p.get_status().await.unwrap();
        assert!(s.position_ms >= 50_000);
    }

    #[tokio::test]
    async fn test_mock_provider_search_and_catalog() {
        let p = MockProvider::new();
        let res = p.search("acoustic", &[], 10, None).await.unwrap();
        assert_eq!(res.tracks.as_ref().unwrap().items.len(), 1);
        assert_eq!(res.tracks.as_ref().unwrap().items[0].id, "mock:track:3");

        let item = p.get_catalog_item("mock:album:1").await.unwrap();
        match item {
            CatalogItemWire::Album(a) => assert_eq!(a.title, "Mock Album A"),
            _ => panic!("Expected album"),
        }

        let tracks = p
            .get_collection_items("mock:album:1", 10, None)
            .await
            .unwrap();
        assert_eq!(tracks.items.len(), 2);

        let lib_albums = p
            .get_library(LibraryKindWire::Albums, 10, None)
            .await
            .unwrap();
        match lib_albums {
            LibraryPageWire::Albums(page) => assert_eq!(page.items.len(), 2),
            _ => panic!("Expected albums"),
        }
    }

    #[tokio::test]
    async fn test_mock_custom_repost_action() {
        let p = MockProvider::new();
        let target = MediaIdWire::parse("mock:track:3").unwrap();
        let res = p
            .custom_action("mock.repost", Some(&target), serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(res["reposted"], true);
        assert_eq!(res["track_id"], "mock:track:3");
        assert_eq!(res["provider"], "mock");

        // Without target
        let res_no_target = p
            .custom_action("mock.repost", None, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(res_no_target["reposted"], true);
        assert!(res_no_target["track_id"].is_null());
    }

    #[test]
    fn test_mock_capabilities_exclude_lyrics() {
        let p = MockProvider::new();
        let caps = p.capabilities();
        assert!(caps.contains(&"search".to_string()));
        assert!(caps.contains(&"catalog.track".to_string()));
        assert!(caps.contains(&"catalog.album".to_string()));
        assert!(caps.contains(&"library.tracks".to_string()));
        assert!(caps.contains(&"playback".to_string()));
        assert!(caps.contains(&"mock.repost".to_string()));
        assert!(caps.contains(&capability::SURFACES.to_string()));
        assert!(!caps.contains(&"lyrics.synced".to_string()));
    }

    #[tokio::test]
    async fn test_mock_surfaces_manifest_and_navigation() {
        let p = MockProvider::new();
        let manifest = p.get_surface_manifest().await.unwrap();
        assert_eq!(manifest.provider_id, "mock");
        assert_eq!(manifest.default_surface_id, "home");
        assert_eq!(manifest.groups.len(), 2);
        assert_eq!(manifest.groups[0].id, "discover");
        assert_eq!(manifest.groups[1].id, "library");
    }

    #[tokio::test]
    async fn test_mock_surface_home_and_shelves() {
        let p = MockProvider::new();
        let home = p.get_surface("home").await.unwrap();
        assert_eq!(home.id, "home");
        assert_eq!(home.title, "Home");
        assert_eq!(home.sections.len(), 2);

        // Section 1: Recently Played shelf
        let s1 = &home.sections[0];
        assert_eq!(s1.id, "recently-played");
        assert_eq!(s1.presentation_hint.as_deref(), Some("shelf"));
        assert_eq!(s1.items.len(), 2);
        assert_eq!(s1.items[0].entity.as_ref().unwrap().kind, "album");
        assert_eq!(
            s1.items[0].open_surface_id.as_deref(),
            Some("album:mock:album:1")
        );

        // Section 2: Discover Weekly shelf with actions and continuation
        let s2 = &home.sections[1];
        assert_eq!(s2.id, "discover-weekly");
        assert_eq!(s2.presentation_hint.as_deref(), Some("shelf"));
        assert_eq!(s2.items.len(), 1);
        assert_eq!(s2.items[0].badges[0].label, "Staff Pick");
        assert_eq!(s2.items[0].actions.len(), 2);
        assert_eq!(s2.items[0].actions[1].role, ActionRoleWire::Toggle);
        assert_eq!(s2.items[0].actions[1].state, Some(ActionStateWire::Active));
        assert!(s2.continuation.is_some());
    }

    #[tokio::test]
    async fn test_mock_surface_continuation() {
        let p = MockProvider::new();
        let cursor = SurfaceCursorWire::section("discover-weekly", "cursor:discover:page2");
        let cont = p.continue_surface("home", &cursor).await.unwrap();
        match cont {
            SurfaceContinuationWire::Section {
                section_id,
                items,
                continuation,
            } => {
                assert_eq!(section_id, "discover-weekly");
                assert_eq!(items.len(), 2);
                assert!(continuation.is_none());
            }
            _ => panic!("Expected section continuation"),
        }
    }

    #[tokio::test]
    async fn test_mock_surface_actions() {
        let p = MockProvider::new();

        // 1. Play action triggers playback
        let res_play = p
            .invoke_surface_action("mock:act:play:mock:track:2")
            .await
            .unwrap();
        assert_eq!(res_play.status, ActionStatusWire::Success);
        let status = p.get_status().await.unwrap();
        assert_eq!(status.state, PlaybackStateWire::Playing);
        assert_eq!(status.current_track.unwrap().id, "mock:track:2");

        // 2. Favorite toggle action triggers CurrentSurface refresh
        let res_fav = p
            .invoke_surface_action("mock:act:fav:mock:track:2")
            .await
            .unwrap();
        assert_eq!(res_fav.status, ActionStatusWire::Success);
        assert_eq!(res_fav.refresh, SurfaceRefreshWire::CurrentSurface);

        // 3. Unknown action fails gracefully
        let res_unknown = p.invoke_surface_action("invalid:token").await.unwrap();
        assert_eq!(res_unknown.status, ActionStatusWire::Failed);
    }

    #[tokio::test]
    async fn test_mock_album_detail_surface() {
        let p = MockProvider::new();
        let surf = p.get_surface("album:mock:album:1").await.unwrap();
        assert_eq!(surf.title, "Mock Album A");
        let header = surf.header.expect("Header must be present");
        assert_eq!(header.title, "Mock Album A");
        assert_eq!(header.badges[0].label, "Lossless Available");
        assert_eq!(header.actions.len(), 2);
        assert_eq!(surf.sections.len(), 1);
        assert_eq!(
            surf.sections[0].presentation_hint.as_deref(),
            Some("track-list")
        );
        assert_eq!(surf.sections[0].items.len(), 2);
    }
}
