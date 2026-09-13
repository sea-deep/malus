//! Apple Music native navigation model.

use malus_ipc::wire::{NavEntryWire, NavGroupWire, NavigationWire};
use malus_model::PageRoute;

/// Build the native Apple Music navigation model for frontends.
pub fn apple_navigation() -> NavigationWire {
    NavigationWire {
        default_route: PageRoute::Home,
        groups: vec![
            NavGroupWire::new(
                "discover",
                Some("Discover".to_string()),
                vec![
                    NavEntryWire::with_icon(PageRoute::Home, "Listen Now", "house"),
                    NavEntryWire::with_icon(PageRoute::New, "Browse", "compass"),
                    NavEntryWire::with_icon(PageRoute::Radio, "Radio", "radio"),
                ],
            ),
            NavGroupWire::new(
                "library",
                Some("Library".to_string()),
                vec![
                    NavEntryWire::with_icon(
                        PageRoute::LibraryRecentlyAdded,
                        "Recently Added",
                        "clock",
                    ),
                    NavEntryWire::with_icon(PageRoute::LibraryArtists, "Artists", "music-mic"),
                    NavEntryWire::with_icon(PageRoute::LibraryAlbums, "Albums", "record-vinyl"),
                    NavEntryWire::with_icon(PageRoute::LibrarySongs, "Songs", "music-note"),
                    NavEntryWire::with_icon(PageRoute::LibraryPlaylists, "Playlists", "music-list"),
                ],
            ),
            NavGroupWire::new(
                "replay",
                Some("Year in Review".to_string()),
                vec![NavEntryWire::with_icon(
                    PageRoute::Replay(2024),
                    "Replay",
                    "replay",
                )],
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_structure() {
        let nav = apple_navigation();
        assert_eq!(nav.default_route, PageRoute::Home);
        assert_eq!(nav.groups.len(), 3);

        // Discover
        assert_eq!(nav.groups[0].id, "discover");
        assert_eq!(nav.groups[0].entries.len(), 3);
        assert_eq!(nav.groups[0].entries[0].route, PageRoute::Home);
        assert_eq!(nav.groups[0].entries[0].label, "Listen Now");

        // Library
        assert_eq!(nav.groups[1].id, "library");
        assert_eq!(nav.groups[1].entries.len(), 5);
        assert_eq!(
            nav.groups[1].entries[0].route,
            PageRoute::LibraryRecentlyAdded
        );

        // Replay
        assert_eq!(nav.groups[2].id, "replay");
        assert_eq!(nav.groups[2].entries[0].route, PageRoute::Replay(2024));
    }

    #[test]
    fn test_navigation_serialization_roundtrip() {
        let nav = apple_navigation();
        let json = serde_json::to_string_pretty(&nav).unwrap();
        let decoded: NavigationWire = serde_json::from_str(&json).unwrap();
        assert_eq!(nav, decoded);
    }
}
