//! Provider-authored surface wire models and transfer objects.
//!
//! Enforces the architectural boundary:
//! - Providers own product taxonomy, navigation, and page semantics.
//! - Protocol carries normalized structural surfaces (manifests, headers, sections, items, actions, continuations).
//! - Malus frontends own native presentation and widgets without service-specific hardcoding.

use crate::wire::ArtworkWire;
use serde::{Deserialize, Serialize};

/// Provider-authored navigation manifest advertising top-level surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSurfaceManifestWire {
    pub provider_id: String,
    pub default_surface_id: String,
    pub groups: Vec<SurfaceNavGroupWire>,
}

/// A grouping of navigation entries (e.g. "Discover", "Library", "Local Files").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceNavGroupWire {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub entries: Vec<SurfaceNavEntryWire>,
}

impl SurfaceNavGroupWire {
    pub fn new(
        id: impl Into<String>,
        title: Option<String>,
        entries: Vec<SurfaceNavEntryWire>,
    ) -> Self {
        Self {
            id: id.into(),
            title,
            entries,
        }
    }
}

/// A specific navigation entry opening an opaque surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceNavEntryWire {
    pub surface_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hint: Option<String>,
}

impl SurfaceNavEntryWire {
    pub fn new(surface_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            surface_id: surface_id.into(),
            label: label.into(),
            icon_hint: None,
        }
    }

    pub fn with_icon(
        surface_id: impl Into<String>,
        label: impl Into<String>,
        icon_hint: impl Into<String>,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            label: label.into(),
            icon_hint: Some(icon_hint.into()),
        }
    }
}

/// A structured, provider-authored surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<SurfaceHeaderWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceActionWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SurfaceSectionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<SurfaceCursorWire>,
}

impl SurfaceWire {
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

/// Optional rich header for album, artist, playlist, or summary surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceHeaderWire {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<SurfaceBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceActionWire>,
}

impl SurfaceHeaderWire {
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

/// An ordered section within a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceSectionWire {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub items: Vec<SurfaceItemWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<SurfaceCursorWire>,
    /// Advisory presentation hint for frontends (e.g. "shelf", "grid", "track-list", "hero").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_hint: Option<String>,
}

impl SurfaceSectionWire {
    pub fn new(id: impl Into<String>, title: Option<String>, items: Vec<SurfaceItemWire>) -> Self {
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

/// A normalized display-oriented surface item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceItemWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tertiary_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<ProviderEntityRefWire>,
    /// If opening this item should navigate directly to another surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<SurfaceBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceActionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_hint: Option<String>,
}

impl SurfaceItemWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            tertiary_text: None,
            artwork: None,
            entity: None,
            open_surface_id: None,
            metadata: Vec::new(),
            badges: Vec::new(),
            actions: Vec::new(),
            presentation_hint: None,
        }
    }
}

/// Opaque provider entity reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderEntityRefWire {
    pub provider_id: String,
    pub id: String,
    /// Extensible provider kind: e.g. "song", "album", "playlist", "station", "apple-curator", "folder".
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

/// Provider-authored badge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceBadgeWire {
    pub label: String,
    /// Advisory style hint: e.g. "explicit", "live", "accent", "lossless".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_hint: Option<String>,
}

impl SurfaceBadgeWire {
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

/// A contextual or standalone provider action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceActionWire {
    /// Opaque action token passed back on invocation.
    pub invocation_token: String,
    pub label: String,
    pub role: ActionRoleWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hint: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ActionStateWire>,
}

impl SurfaceActionWire {
    pub fn new(
        invocation_token: impl Into<String>,
        label: impl Into<String>,
        role: ActionRoleWire,
    ) -> Self {
        Self {
            invocation_token: invocation_token.into(),
            label: label.into(),
            role,
            icon_hint: None,
            enabled: true,
            state: None,
        }
    }

    pub fn toggle(
        invocation_token: impl Into<String>,
        label: impl Into<String>,
        state: ActionStateWire,
    ) -> Self {
        Self {
            invocation_token: invocation_token.into(),
            label: label.into(),
            role: ActionRoleWire::Toggle,
            icon_hint: None,
            enabled: true,
            state: Some(state),
        }
    }
}

/// Action interaction role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionRoleWire {
    Primary,
    Secondary,
    Toggle,
    Destructive,
    Context,
}

/// Action toggle / active state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionStateWire {
    Inactive,
    Active,
    Mixed,
}

/// Result of an action invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceActionResultWire {
    pub status: ActionStatusWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub refresh: SurfaceRefreshWire,
}

impl SurfaceActionResultWire {
    pub fn success() -> Self {
        Self {
            status: ActionStatusWire::Success,
            message: None,
            refresh: SurfaceRefreshWire::None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: ActionStatusWire::Failed,
            message: Some(message.into()),
            refresh: SurfaceRefreshWire::None,
        }
    }

    pub fn with_refresh(mut self, refresh: SurfaceRefreshWire) -> Self {
        self.refresh = refresh;
        self
    }
}

/// Status of an action execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionStatusWire {
    Success,
    Failed,
}

/// Declarative refresh policy after action execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "surfaces", rename_all = "kebab-case")]
pub enum SurfaceRefreshWire {
    #[default]
    None,
    CurrentSurface,
    SpecificSurfaces(Vec<String>),
}

/// Opaque pagination cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCursorWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    pub token: String,
}

impl SurfaceCursorWire {
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

/// Result of continuing a surface or section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SurfaceContinuationWire {
    Section {
        section_id: String,
        items: Vec<SurfaceItemWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<SurfaceCursorWire>,
    },
    Sections {
        sections: Vec<SurfaceSectionWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<SurfaceCursorWire>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_manifest_roundtrip() {
        let manifest = ProviderSurfaceManifestWire {
            provider_id: "apple".to_string(),
            default_surface_id: "home".to_string(),
            groups: vec![
                SurfaceNavGroupWire::new(
                    "discover",
                    Some("Discover".to_string()),
                    vec![
                        SurfaceNavEntryWire::with_icon("home", "Home", "house"),
                        SurfaceNavEntryWire::with_icon("browse", "Browse", "compass"),
                        SurfaceNavEntryWire::with_icon("radio", "Radio", "radio"),
                    ],
                ),
                SurfaceNavGroupWire::new(
                    "library",
                    Some("Library".to_string()),
                    vec![
                        SurfaceNavEntryWire::with_icon(
                            "library/recently-added",
                            "Recently Added",
                            "clock",
                        ),
                        SurfaceNavEntryWire::with_icon("library/artists", "Artists", "music-mic"),
                        SurfaceNavEntryWire::with_icon("library/albums", "Albums", "record-vinyl"),
                        SurfaceNavEntryWire::with_icon("library/songs", "Songs", "music-note"),
                        SurfaceNavEntryWire::with_icon(
                            "library/playlists",
                            "Playlists",
                            "music-list",
                        ),
                    ],
                ),
            ],
        };

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let decoded: ProviderSurfaceManifestWire = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, decoded);
    }

    #[test]
    fn test_cross_provider_models_apple_vs_local() {
        // 1. Apple-like surface representation
        let mut apple_home = SurfaceWire::new("home", "Listen Now");
        apple_home.subtitle = Some("Personal recommendations and radio".to_string());

        let mut rec_shelf = SurfaceSectionWire::new(
            "shelf:made-for-you",
            Some("Made For You".to_string()),
            vec![{
                let mut item = SurfaceItemWire::new("pl.fav-mix", "Favorites Mix");
                item.subtitle = Some("Updated Tuesdays".to_string());
                item.entity = Some(ProviderEntityRefWire::new(
                    "apple",
                    "pl.fav-mix",
                    "playlist",
                ));
                item.open_surface_id = Some("apple:surface:playlist:pl.fav-mix".to_string());
                item.badges = vec![SurfaceBadgeWire::new("Apple Music")];
                item.actions = vec![SurfaceActionWire::new(
                    "tok:play:pl.fav-mix",
                    "Play",
                    ActionRoleWire::Primary,
                )];
                item.presentation_hint = Some("square-card".to_string());
                item
            }],
        )
        .with_hint("shelf");
        rec_shelf.continuation = Some(SurfaceCursorWire::section(
            "shelf:made-for-you",
            "cursor-offset-10",
        ));

        apple_home.sections.push(rec_shelf);

        let apple_json = serde_json::to_string(&apple_home).unwrap();
        let apple_decoded: SurfaceWire = serde_json::from_str(&apple_json).unwrap();
        assert_eq!(apple_home, apple_decoded);
        assert_eq!(
            apple_decoded.sections[0].presentation_hint.as_deref(),
            Some("shelf")
        );
        assert_eq!(
            apple_decoded.sections[0].items[0]
                .entity
                .as_ref()
                .unwrap()
                .kind,
            "playlist"
        );

        // 2. Local-folder-like surface representation
        let mut local_folder = SurfaceWire::new("folder:/Music/FLAC", "/Music/FLAC");
        let folder_section = SurfaceSectionWire::new(
            "dir-contents",
            Some("Directory Contents".to_string()),
            vec![
                {
                    let mut item = SurfaceItemWire::new("item:subfolder:Electronic", "Electronic");
                    item.subtitle = Some("Subfolder • 24 items".to_string());
                    item.entity = Some(ProviderEntityRefWire::new(
                        "local",
                        "/Music/FLAC/Electronic",
                        "folder",
                    ));
                    item.open_surface_id =
                        Some("local:surface:folder:/Music/FLAC/Electronic".to_string());
                    item
                },
                {
                    let mut item =
                        SurfaceItemWire::new("item:track:01-intro.flac", "01-intro.flac");
                    item.subtitle = Some("FLAC • 44.1kHz 16-bit • 4.2 MB".to_string());
                    item.entity = Some(ProviderEntityRefWire::new(
                        "local",
                        "/Music/FLAC/01-intro.flac",
                        "local-file",
                    ));
                    item.actions = vec![SurfaceActionWire::new(
                        "tok:play:/Music/FLAC/01-intro.flac",
                        "Play",
                        ActionRoleWire::Primary,
                    )];
                    item
                },
            ],
        )
        .with_hint("compact-list");

        local_folder.sections.push(folder_section);

        let local_json = serde_json::to_string(&local_folder).unwrap();
        let local_decoded: SurfaceWire = serde_json::from_str(&local_json).unwrap();
        assert_eq!(local_folder, local_decoded);
        assert_eq!(
            local_decoded.sections[0].presentation_hint.as_deref(),
            Some("compact-list")
        );
        assert_eq!(
            local_decoded.sections[0].items[0]
                .entity
                .as_ref()
                .unwrap()
                .kind,
            "folder"
        );
        assert_eq!(
            local_decoded.sections[0].items[1]
                .entity
                .as_ref()
                .unwrap()
                .kind,
            "local-file"
        );
    }

    #[test]
    fn test_presentation_hint_extensibility() {
        let section =
            SurfaceSectionWire::new("test", None, vec![]).with_hint("futuristic-3d-carousel");
        let json = serde_json::to_string(&section).unwrap();
        let decoded: SurfaceSectionWire = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded.presentation_hint.as_deref(),
            Some("futuristic-3d-carousel")
        );
    }

    #[test]
    fn test_provider_entity_kind_extensibility() {
        let entity =
            ProviderEntityRefWire::new("custom_service", "custom_id_999", "custom-arbitrary-kind");
        let json = serde_json::to_string(&entity).unwrap();
        let decoded: ProviderEntityRefWire = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.kind, "custom-arbitrary-kind");
    }

    #[test]
    fn test_opaque_tokens_and_refresh() {
        let action = SurfaceActionWire::toggle(
            "tok:opaque-internal-state-12345",
            "Favorite",
            ActionStateWire::Active,
        );
        let act_json = serde_json::to_string(&action).unwrap();
        let act_dec: SurfaceActionWire = serde_json::from_str(&act_json).unwrap();
        assert_eq!(act_dec.invocation_token, "tok:opaque-internal-state-12345");
        assert_eq!(act_dec.state, Some(ActionStateWire::Active));

        let res =
            SurfaceActionResultWire::success().with_refresh(SurfaceRefreshWire::SpecificSurfaces(
                vec!["home".to_string(), "library/favorites".to_string()],
            ));
        let res_json = serde_json::to_string(&res).unwrap();
        let res_dec: SurfaceActionResultWire = serde_json::from_str(&res_json).unwrap();
        assert_eq!(res_dec.status, ActionStatusWire::Success);
        match res_dec.refresh {
            SurfaceRefreshWire::SpecificSurfaces(surfaces) => {
                assert_eq!(
                    surfaces,
                    vec!["home".to_string(), "library/favorites".to_string()]
                );
            }
            _ => panic!("Expected SpecificSurfaces"),
        }
    }

    #[test]
    fn test_continuation_wire_roundtrip() {
        let cont = SurfaceContinuationWire::Section {
            section_id: "sec-1".to_string(),
            items: vec![SurfaceItemWire::new("item-2", "Item 2")],
            continuation: Some(SurfaceCursorWire::section("sec-1", "tok-next")),
        };
        let json = serde_json::to_string(&cont).unwrap();
        let decoded: SurfaceContinuationWire = serde_json::from_str(&json).unwrap();
        assert_eq!(cont, decoded);
    }
}
