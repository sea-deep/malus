//! Persistent library sort configuration.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;

/// Which field library songs are sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySortField {
    #[default]
    #[serde(alias = "default")]
    RecentlyAdded,
    Title,
    Artist,
    Album,
    Duration,
}

impl LibrarySortField {
    pub const ALL: [Self; 5] = [
        Self::RecentlyAdded,
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::Duration,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "Recently Added",
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::Duration => "Duration",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::RecentlyAdded => Self::Title,
            Self::Title => Self::Artist,
            Self::Artist => Self::Album,
            Self::Album => Self::Duration,
            Self::Duration => Self::RecentlyAdded,
        }
    }

    /// Natural default sort order for this field.
    pub fn default_order(self) -> LibrarySortOrder {
        match self {
            Self::RecentlyAdded => LibrarySortOrder::Desc,
            _ => LibrarySortOrder::Asc,
        }
    }
}

/// Ascending or descending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySortOrder {
    #[default]
    Desc,
    Asc,
}

impl LibrarySortOrder {
    pub fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Asc => "▲",
            Self::Desc => "▼",
        }
    }
}

/// Complete persistent sort preference for the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LibrarySort {
    pub field: LibrarySortField,
    pub order: LibrarySortOrder,
}

impl LibrarySort {
    /// Load sort configuration from `~/.config/malus/sort.toml`.
    pub fn load() -> Self {
        if let Some(proj_dirs) = ProjectDirs::from("com", "malus", "malus") {
            let path = proj_dirs.config_dir().join("sort.toml");
            if let Ok(content) = fs::read_to_string(&path)
                && let Ok(sort) = toml::from_str(&content)
            {
                return sort;
            }
        }
        Self::default()
    }

    /// Save sort configuration to `~/.config/malus/sort.toml`.
    pub fn save(&self) {
        if let Some(proj_dirs) = ProjectDirs::from("com", "malus", "malus") {
            let dir = proj_dirs.config_dir();
            let _ = fs::create_dir_all(dir);
            let path = dir.join("sort.toml");
            if let Ok(content) = toml::to_string_pretty(self) {
                let _ = fs::write(path, content);
            }
        }
    }
}
