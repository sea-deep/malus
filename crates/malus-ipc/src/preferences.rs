//! Persistent player preferences across daemon and client sessions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerPreferences {
    #[serde(default = "default_autoplay")]
    pub autoplay: bool,
}

fn default_autoplay() -> bool {
    true
}

impl Default for PlayerPreferences {
    fn default() -> Self {
        Self {
            autoplay: default_autoplay(),
        }
    }
}

impl PlayerPreferences {
    pub fn config_path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("malus/preferences.json"))
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Err(std::io::Error::other(
                "No configuration directory is available",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_preferences_default() {
        let pref = PlayerPreferences::default();
        assert!(pref.autoplay);
    }

    #[test]
    fn test_player_preferences_serde() {
        let json = r#"{"autoplay": false}"#;
        let pref: PlayerPreferences = serde_json::from_str(json).unwrap();
        assert!(!pref.autoplay);

        let json_empty = r#"{}"#;
        let pref_empty: PlayerPreferences = serde_json::from_str(json_empty).unwrap();
        assert!(pref_empty.autoplay);
    }
}
