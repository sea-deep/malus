//! Page, navigation, entity, and action wire models.
//!
//! Direct native Apple Music model for Linux:
//! - Pages represent Apple-authored dynamic feeds (Home, New/Browse, Radio, Library, Details, Replay).
//! - Typed routes (`PageRoute`) identify pages.
//! - Media references (`MediaRef`) identify Apple resources.
//! - Typed actions (`PageActionWire`) declare playback and queue operations.

pub use malus_model::PageRoute;
use malus_model::{Artwork, MediaRef};
use serde::{Deserialize, Serialize};

/// Navigation hierarchy for the native Apple Music client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationWire {
    pub default_route: PageRoute,
    pub groups: Vec<NavGroupWire>,
}

impl NavigationWire {
    pub fn new(default_route: PageRoute, groups: Vec<NavGroupWire>) -> Self {
        Self {
            default_route,
            groups,
        }
    }
}

/// Navigation section grouping (e.g. "Discover", "Library").
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

/// A specific navigation entry opening a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavEntryWire {
    pub route: PageRoute,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hint: Option<String>,
}

impl NavEntryWire {
    pub fn new(route: PageRoute, label: impl Into<String>) -> Self {
        Self {
            route,
            label: label.into(),
            icon_hint: None,
        }
    }

    pub fn with_icon(
        route: PageRoute,
        label: impl Into<String>,
        icon_hint: impl Into<String>,
    ) -> Self {
        Self {
            route,
            label: label.into(),
            icon_hint: Some(icon_hint.into()),
        }
    }
}

/// A structured page returned to frontends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<PageHeaderWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PageActionWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<PageSectionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<PageCursorWire>,
}

impl PageWire {
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
    pub artwork: Option<Artwork>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<PageBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PageActionWire>,
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
    pub artwork: Option<Artwork>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<MediaRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_route: Option<PageRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<PageBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PageActionWire>,
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

/// Display badge (e.g. "Explicit", "Live", "Lossless").
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

/// Typed actions for playback, queue, and account operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "target")]
pub enum PageActionWire {
    Play(MediaRef),
    PlayNext(MediaRef),
    PlayLater(MediaRef),
    Favorite(MediaRef),
    Unfavorite(MediaRef),
    SuggestLess(MediaRef),
    AddToLibrary(MediaRef),
}

/// Pagination cursor for continuing a page or section.
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

/// Result of continuing a page or section.
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
    SpecificRoutes(Vec<PageRoute>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_wire_roundtrip() {
        let mut page = PageWire::new("home", "Listen Now");
        page.subtitle = Some("Recommendations and radio".to_string());

        let mut item = PageItemWire::new("item-1", "Daft Punk Essentials");
        item.subtitle = Some("Apple Music Electronic".to_string());
        item.entity = Some(MediaRef::parse("playlist:pl.123").unwrap());
        item.open_route = Some(PageRoute::Playlist("pl.123".to_string()));
        item.actions = vec![PageActionWire::Play(
            MediaRef::parse("playlist:pl.123").unwrap(),
        )];
        item.badges = vec![PageBadgeWire::new("Essential")];

        let section = PageSectionWire::new("shelf-1", Some("Featured".to_string()), vec![item])
            .with_hint("shelf");
        page.sections.push(section);

        let json = serde_json::to_string_pretty(&page).unwrap();
        let decoded: PageWire = serde_json::from_str(&json).unwrap();
        assert_eq!(page, decoded);
    }

    #[test]
    fn test_navigation_roundtrip() {
        let nav = NavigationWire::new(
            PageRoute::Home,
            vec![NavGroupWire::new(
                "discover",
                Some("Discover".to_string()),
                vec![
                    NavEntryWire::with_icon(PageRoute::Home, "Listen Now", "house"),
                    NavEntryWire::with_icon(PageRoute::New, "Browse", "compass"),
                ],
            )],
        );
        let json = serde_json::to_string(&nav).unwrap();
        let decoded: NavigationWire = serde_json::from_str(&json).unwrap();
        assert_eq!(nav, decoded);
    }
}
