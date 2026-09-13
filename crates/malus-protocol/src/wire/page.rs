//! Apple-first page, navigation, entity, and action wire models.
//!
//! Replaces the generic multi-provider surface framework with a direct,
//! native Apple Music model for Linux:
//! - Pages represent Apple-authored dynamic feeds (Home, New/Browse, Radio, Library, Details, Replay).
//! - Typed Apple routes (`ApplePageRoute`) identify pages.
//! - Typed Apple entity references (`AppleEntityRefWire`) identify Apple resources without `provider_id`.
//! - Typed Apple actions (`AppleActionWire`) declare playback, queue, and mutation operations.

use crate::wire::ArtworkWire;
use serde::{Deserialize, Serialize};

/// Navigation hierarchy for the native Apple Music client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleNavigationWire {
    pub default_route: String,
    pub groups: Vec<NavGroupWire>,
}

impl AppleNavigationWire {
    pub fn new(default_route: impl Into<String>, groups: Vec<NavGroupWire>) -> Self {
        Self {
            default_route: default_route.into(),
            groups,
        }
    }
}

/// Navigation section grouping (e.g. "Discover", "Library", "Replay").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavGroupWire {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub entries: Vec<NavEntryWire>,
}

impl NavGroupWire {
    pub fn new(id: impl Into<String>, title: Option<String>, entries: Vec<NavEntryWire>) -> Self {
        Self {
            id: id.into(),
            title,
            entries,
        }
    }
}

/// A specific navigation entry opening an Apple page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavEntryWire {
    pub route: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hint: Option<String>,
}

impl NavEntryWire {
    pub fn new(route: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            route: route.into(),
            label: label.into(),
            icon_hint: None,
        }
    }

    pub fn with_icon(
        route: impl Into<String>,
        label: impl Into<String>,
        icon_hint: impl Into<String>,
    ) -> Self {
        Self {
            route: route.into(),
            label: label.into(),
            icon_hint: Some(icon_hint.into()),
        }
    }
}

/// Typed Apple page route representing all consumer destinations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "kebab-case")]
pub enum ApplePageRoute {
    Home,
    New,
    Radio,
    LibraryRecentlyAdded,
    LibrarySongs,
    LibraryAlbums,
    LibraryArtists,
    LibraryPlaylists,
    Album(String),
    Artist(String),
    Playlist(String),
    Replay(String),
}

impl ApplePageRoute {
    /// Parse from a canonical route string (e.g. "apple:page:home" or "home").
    pub fn parse(route: &str) -> Option<Self> {
        let clean = route
            .strip_prefix("apple:page:")
            .or_else(|| route.strip_prefix("apple:surface:"))
            .unwrap_or(route);

        match clean {
            "home" => Some(Self::Home),
            "new" | "browse" => Some(Self::New),
            "radio" => Some(Self::Radio),
            "library/recently-added" | "library:recently-added" => Some(Self::LibraryRecentlyAdded),
            "library/songs" | "library:songs" => Some(Self::LibrarySongs),
            "library/albums" | "library:albums" => Some(Self::LibraryAlbums),
            "library/artists" | "library:artists" => Some(Self::LibraryArtists),
            "library/playlists" | "library:playlists" => Some(Self::LibraryPlaylists),
            _ => {
                if let Some(id) = clean.strip_prefix("album:")
                    && !id.is_empty()
                {
                    return Some(Self::Album(id.to_string()));
                }
                if let Some(id) = clean.strip_prefix("artist:")
                    && !id.is_empty()
                {
                    return Some(Self::Artist(id.to_string()));
                }
                if let Some(id) = clean.strip_prefix("playlist:")
                    && !id.is_empty()
                {
                    return Some(Self::Playlist(id.to_string()));
                }
                if let Some(year) = clean.strip_prefix("replay:")
                    && !year.is_empty()
                {
                    return Some(Self::Replay(year.to_string()));
                }
                None
            }
        }
    }

    /// Format to canonical route string.
    pub fn format(&self) -> String {
        match self {
            Self::Home => "apple:page:home".to_string(),
            Self::New => "apple:page:new".to_string(),
            Self::Radio => "apple:page:radio".to_string(),
            Self::LibraryRecentlyAdded => "apple:page:library:recently-added".to_string(),
            Self::LibrarySongs => "apple:page:library:songs".to_string(),
            Self::LibraryAlbums => "apple:page:library:albums".to_string(),
            Self::LibraryArtists => "apple:page:library:artists".to_string(),
            Self::LibraryPlaylists => "apple:page:library:playlists".to_string(),
            Self::Album(id) => format!("apple:page:album:{id}"),
            Self::Artist(id) => format!("apple:page:artist:{id}"),
            Self::Playlist(id) => format!("apple:page:playlist:{id}"),
            Self::Replay(year) => format!("apple:page:replay:{year}"),
        }
    }
}

/// A structured Apple Music page returned to frontends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplePageWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<PageHeaderWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AppleActionWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<PageSectionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<PageCursorWire>,
}

impl ApplePageWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            header: None,
            actions: Vec::new(),
            sections: Vec::new(),
            continuation: None,
        }
    }
}

/// Rich header for album, artist, playlist, or summary pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageHeaderWire {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<PageBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AppleActionWire>,
}

impl PageHeaderWire {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            artwork: None,
            metadata: Vec::new(),
            badges: Vec::new(),
            actions: Vec::new(),
        }
    }
}

/// An ordered section within a page (e.g. shelf, track list, grid).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSectionWire {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub items: Vec<PageItemWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<PageCursorWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_hint: Option<String>,
}

impl PageSectionWire {
    pub fn new(id: impl Into<String>, title: Option<String>, items: Vec<PageItemWire>) -> Self {
        Self {
            id: id.into(),
            title,
            subtitle: None,
            items,
            continuation: None,
            presentation_hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.presentation_hint = Some(hint.into());
        self
    }
}

/// A normalized display-oriented page item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageItemWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tertiary_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<AppleEntityRefWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_route: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<PageBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AppleActionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_hint: Option<String>,
}

impl PageItemWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            tertiary_text: None,
            artwork: None,
            entity: None,
            open_route: None,
            metadata: Vec::new(),
            badges: Vec::new(),
            actions: Vec::new(),
            presentation_hint: None,
        }
    }
}

/// Typed Apple Music entity identity without generic provider strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "kebab-case")]
pub enum AppleEntityRefWire {
    Song(String),
    Album(String),
    Artist(String),
    Playlist(String),
    Station(String),
    Curator(String),
}

impl AppleEntityRefWire {
    pub fn raw_id(&self) -> &str {
        match self {
            Self::Song(id)
            | Self::Album(id)
            | Self::Artist(id)
            | Self::Playlist(id)
            | Self::Station(id)
            | Self::Curator(id) => id,
        }
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Song(_) => "song",
            Self::Album(_) => "album",
            Self::Artist(_) => "artist",
            Self::Playlist(_) => "playlist",
            Self::Station(_) => "station",
            Self::Curator(_) => "curator",
        }
    }
}

/// Display badge (e.g. "Explicit", "Lossless", "Apple Music 1").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageBadgeWire {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_hint: Option<String>,
}

impl PageBadgeWire {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style_hint: None,
        }
    }

    pub fn with_style(label: impl Into<String>, style: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style_hint: Some(style.into()),
        }
    }
}

/// Typed Apple Music actions for playback, queueing, and collection mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "kebab-case")]
pub enum AppleActionWire {
    PlaySong(String),
    PlayAlbum(String),
    PlayPlaylist(String),
    PlayStation(String),
    PlayNext(String),
    PlayLater(String),
    Favorite(String),
    Unfavorite(String),
    SuggestLess(String),
    AddToLibrary(String),
}

/// Pagination cursor for continuing an Apple page or section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageCursorWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    pub token: String,
}

impl PageCursorWire {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            section_id: None,
            token: token.into(),
        }
    }

    pub fn section(section_id: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            section_id: Some(section_id.into()),
            token: token.into(),
        }
    }
}

/// Result of continuing an Apple page or section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PageContinuationWire {
    Section {
        section_id: String,
        items: Vec<PageItemWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<PageCursorWire>,
    },
    Sections {
        sections: Vec<PageSectionWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<PageCursorWire>,
    },
}

/// Result of executing an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResultWire {
    pub status: ActionStatusWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub refresh: PageRefreshWire,
}

impl ActionResultWire {
    pub fn success() -> Self {
        Self {
            status: ActionStatusWire::Success,
            message: None,
            refresh: PageRefreshWire::None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: ActionStatusWire::Failed,
            message: Some(message.into()),
            refresh: PageRefreshWire::None,
        }
    }

    pub fn with_refresh(mut self, refresh: PageRefreshWire) -> Self {
        self.refresh = refresh;
        self
    }
}

/// Action execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionStatusWire {
    Success,
    Failed,
}

/// Refresh directive after action execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "routes", rename_all = "kebab-case")]
pub enum PageRefreshWire {
    #[default]
    None,
    CurrentPage,
    SpecificRoutes(Vec<String>),
    #[serde(rename = "current-surface")]
    CurrentSurface,
    #[serde(rename = "specific-surfaces")]
    SpecificSurfaces(Vec<String>),
}

// Backward compatibility type aliases
pub type ProviderSurfaceManifestWire = AppleNavigationWire;
pub type SurfaceNavGroupWire = NavGroupWire;
pub type SurfaceNavEntryWire = NavEntryWire;
pub type SurfaceWire = ApplePageWire;
pub type SurfaceHeaderWire = PageHeaderWire;
pub type SurfaceSectionWire = PageSectionWire;
pub type SurfaceItemWire = PageItemWire;
pub type SurfaceBadgeWire = PageBadgeWire;
pub type SurfaceCursorWire = PageCursorWire;
pub type SurfaceContinuationWire = PageContinuationWire;
pub type SurfaceActionResultWire = ActionResultWire;
pub type SurfaceRefreshWire = PageRefreshWire;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEntityRefWire {
    pub provider_id: String,
    pub id: String,
    pub kind: String,
}

impl ProviderEntityRefWire {
    pub fn new(
        provider_id: impl Into<String>,
        id: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            id: id.into(),
            kind: kind.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceActionWire {
    pub invocation_token: String,
    pub label: String,
    pub role: ActionRoleWire,
}

impl SurfaceActionWire {
    pub fn new(token: impl Into<String>, label: impl Into<String>, role: ActionRoleWire) -> Self {
        Self {
            invocation_token: token.into(),
            label: label.into(),
            role,
        }
    }

    pub fn toggle(
        token: impl Into<String>,
        label: impl Into<String>,
        _state: ActionStateWire,
    ) -> Self {
        Self {
            invocation_token: token.into(),
            label: label.into(),
            role: ActionRoleWire::Toggle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionRoleWire {
    Primary,
    Secondary,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStateWire {
    Inactive,
    Active,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apple_page_route_parse_and_format() {
        assert_eq!(ApplePageRoute::parse("home"), Some(ApplePageRoute::Home));
        assert_eq!(
            ApplePageRoute::parse("apple:page:home"),
            Some(ApplePageRoute::Home)
        );
        assert_eq!(
            ApplePageRoute::parse("apple:surface:home"),
            Some(ApplePageRoute::Home)
        );
        assert_eq!(
            ApplePageRoute::parse("apple:page:new"),
            Some(ApplePageRoute::New)
        );
        assert_eq!(
            ApplePageRoute::parse("apple:page:radio"),
            Some(ApplePageRoute::Radio)
        );
        assert_eq!(
            ApplePageRoute::parse("apple:page:album:123"),
            Some(ApplePageRoute::Album("123".to_string()))
        );
        assert_eq!(
            ApplePageRoute::parse("apple:page:artist:456"),
            Some(ApplePageRoute::Artist("456".to_string()))
        );
        assert_eq!(
            ApplePageRoute::parse("apple:page:playlist:pl.789"),
            Some(ApplePageRoute::Playlist("pl.789".to_string()))
        );
        assert_eq!(
            ApplePageRoute::parse("apple:page:replay:2026"),
            Some(ApplePageRoute::Replay("2026".to_string()))
        );

        assert_eq!(ApplePageRoute::Home.format(), "apple:page:home");
        assert_eq!(
            ApplePageRoute::Album("123".to_string()).format(),
            "apple:page:album:123"
        );
    }

    #[test]
    fn test_apple_page_wire_roundtrip() {
        let mut page = ApplePageWire::new("apple:page:home", "Listen Now");
        page.subtitle = Some("Recommendations and radio".to_string());

        let mut item = PageItemWire::new("item-1", "Daft Punk Essentials");
        item.subtitle = Some("Apple Music Electronic".to_string());
        item.entity = Some(AppleEntityRefWire::Playlist("pl.123".to_string()));
        item.open_route = Some("apple:page:playlist:pl.123".to_string());
        item.actions = vec![AppleActionWire::PlayPlaylist("pl.123".to_string())];
        item.badges = vec![PageBadgeWire::new("Essential")];

        let section = PageSectionWire::new("shelf-1", Some("Featured".to_string()), vec![item])
            .with_hint("shelf");
        page.sections.push(section);

        let json = serde_json::to_string_pretty(&page).unwrap();
        let decoded: ApplePageWire = serde_json::from_str(&json).unwrap();
        assert_eq!(page, decoded);
    }

    #[test]
    fn test_apple_navigation_roundtrip() {
        let nav = AppleNavigationWire::new(
            "apple:page:home",
            vec![NavGroupWire::new(
                "discover",
                Some("Discover".to_string()),
                vec![
                    NavEntryWire::with_icon("apple:page:home", "Listen Now", "house"),
                    NavEntryWire::with_icon("apple:page:new", "Browse", "compass"),
                ],
            )],
        );
        let json = serde_json::to_string(&nav).unwrap();
        let decoded: AppleNavigationWire = serde_json::from_str(&json).unwrap();
        assert_eq!(nav, decoded);
    }
}
