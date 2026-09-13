//! Apple Music native navigation model.

use malus_protocol::wire::{AppleNavigationWire, NavEntryWire, NavGroupWire};

/// Build the native Apple Music navigation model for frontends.
pub fn apple_navigation() -> AppleNavigationWire {
    AppleNavigationWire {
        default_route: "apple:page:home".to_string(),
        groups: vec![
            NavGroupWire::new(
                "discover",
                Some("Discover".to_string()),
                vec![
                    NavEntryWire::with_icon("apple:page:home", "Listen Now", "house"),
                    NavEntryWire::with_icon("apple:page:new", "Browse", "compass"),
                    NavEntryWire::with_icon("apple:page:radio", "Radio", "radio"),
                ],
            ),
            NavGroupWire::new(
                "library",
                Some("Library".to_string()),
                vec![
                    NavEntryWire::with_icon(
                        "apple:page:library:recently-added",
                        "Recently Added",
                        "clock",
                    ),
                    NavEntryWire::with_icon("apple:page:library:artists", "Artists", "music-mic"),
                    NavEntryWire::with_icon("apple:page:library:albums", "Albums", "record-vinyl"),
                    NavEntryWire::with_icon("apple:page:library:songs", "Songs", "music-note"),
                    NavEntryWire::with_icon(
                        "apple:page:library:playlists",
                        "Playlists",
                        "music-list",
                    ),
                ],
            ),
            NavGroupWire::new(
                "replay",
                Some("Year in Review".to_string()),
                vec![NavEntryWire::with_icon(
                    "apple:page:replay:latest",
                    "Replay",
                    "replay",
                )],
            ),
        ],
    }
}

// Backwards compatibility alias
pub fn apple_surface_manifest() -> AppleNavigationWire {
    apple_navigation()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_structure() {
        let nav = apple_navigation();
        assert_eq!(nav.default_route, "apple:page:home");
        assert_eq!(nav.groups.len(), 3);

        // Discover
        assert_eq!(nav.groups[0].id, "discover");
        assert_eq!(nav.groups[0].entries.len(), 3);
        assert_eq!(nav.groups[0].entries[0].route, "apple:page:home");
        assert_eq!(nav.groups[0].entries[0].label, "Listen Now");

        // Library
        assert_eq!(nav.groups[1].id, "library");
        assert_eq!(nav.groups[1].entries.len(), 5);

        // Replay
        assert_eq!(nav.groups[2].id, "replay");
        assert_eq!(nav.groups[2].entries[0].route, "apple:page:replay:latest");
    }

    #[test]
    fn test_navigation_serialization_roundtrip() {
        let nav = apple_navigation();
        let json = serde_json::to_string_pretty(&nav).unwrap();
        let decoded: AppleNavigationWire = serde_json::from_str(&json).unwrap();
        assert_eq!(nav, decoded);
    }
}
