//! Modular keymap engine for Malus.
//!
//! Provides decoupled key action mappings, chord parsing, and optional TOML configuration
//! loaded from `~/.config/malus/keymap.toml`.

use directories::ProjectDirs;
use ratcn::runtime::{KeyCode, KeyEvent};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, fmt, fs, path::Path, str::FromStr};

/// All discrete user actions dispatchable via keyboard shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // Playback
    TogglePlay,
    NextTrack,
    PrevTrack,
    VolumeUp,
    VolumeDown,
    ToggleShuffle,
    CycleRepeat,

    // Navigation
    NavigateListenNow,
    NavigateBrowse,
    NavigateRadio,
    NavigateLibrary,
    NavigateNowPlaying,
    NavigateBack,

    // Overlays
    ToggleLyrics,
    ToggleQueue,
    OpenSearch,
    ToggleCommandPalette,
    ToggleHelp,
    ToggleSettings,
    CloseOverlay,
    Escape,

    // Selection & List navigation
    SelectUp,
    SelectDown,
    PageUp,
    PageDown,
    SelectFirst,
    SelectLast,
    Activate,

    // Queue mutations
    QueueMoveUp,
    QueueMoveDown,
    QueueDelete,

    // Text input helpers
    SearchClear,
    SearchBackspace,
    CommandBackspace,

    // Application lifecycle
    Refresh,
    Quit,
}

/// A combination of a key and modifier keys (Ctrl, Alt, Shift).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyChord {
    pub const fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Check if a live runtime KeyEvent matches this chord.
    pub fn matches(&self, key: &KeyEvent) -> bool {
        if self.ctrl != key.modifiers.ctrl || self.alt != key.modifiers.alt {
            return false;
        }

        match (self.code, key.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => {
                if self.ctrl || self.alt {
                    a.eq_ignore_ascii_case(&b)
                } else if self.shift {
                    a == b || (a.is_alphabetic() && a.to_ascii_uppercase() == b)
                } else {
                    a == b
                }
            }
            (a, b) => a == b,
        }
    }
}

impl From<KeyCode> for KeyChord {
    fn from(code: KeyCode) -> Self {
        Self::new(code)
    }
}

impl From<char> for KeyChord {
    fn from(c: char) -> Self {
        Self::new(KeyCode::Char(c))
    }
}

impl FromStr for KeyChord {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Empty key chord".to_string());
        }

        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;

        let parts: Vec<&str> = s.split('+').collect();
        if parts.is_empty() {
            return Err("Empty key chord".to_string());
        }

        // Handle case where "+" itself is the key (e.g. "ctrl++" or "+")
        let (mods, key_part) = if parts.len() > 1 && parts.last().unwrap().is_empty() {
            // Ending with '+' means the key itself was '+'
            (&parts[..parts.len() - 2], "+")
        } else {
            (&parts[..parts.len() - 1], *parts.last().unwrap())
        };

        for m in mods {
            match m.trim().to_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "opt" | "option" => alt = true,
                "shift" => shift = true,
                other => return Err(format!("Unknown modifier: {other}")),
            }
        }

        let key_str = key_part.trim();
        let code = match key_str.to_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "esc" | "escape" => KeyCode::Esc,
            "enter" | "return" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "pageup" | "page_up" => KeyCode::PageUp,
            "pagedown" | "page_down" => KeyCode::PageDown,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            s if s.starts_with('f') && s[1..].parse::<u8>().is_ok() => {
                let n: u8 = s[1..].parse().unwrap();
                KeyCode::F(n)
            }
            _ => {
                let mut chars = key_str.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    KeyCode::Char(c)
                } else {
                    return Err(format!("Unknown key: {key_str}"));
                }
            }
        };

        Ok(KeyChord {
            code,
            ctrl,
            alt,
            shift,
        })
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            write!(f, "Ctrl+")?;
        }
        if self.alt {
            write!(f, "Alt+")?;
        }
        if self.shift {
            write!(f, "Shift+")?;
        }
        match self.code {
            KeyCode::Char(' ') => write!(f, "Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Esc => write!(f, "Esc"),
            KeyCode::Enter => write!(f, "Enter"),
            KeyCode::Tab => write!(f, "Tab"),
            KeyCode::BackTab => write!(f, "BackTab"),
            KeyCode::Backspace => write!(f, "Backspace"),
            KeyCode::Delete => write!(f, "Delete"),
            KeyCode::Up => write!(f, "Up"),
            KeyCode::Down => write!(f, "Down"),
            KeyCode::Left => write!(f, "Left"),
            KeyCode::Right => write!(f, "Right"),
            KeyCode::PageUp => write!(f, "PageUp"),
            KeyCode::PageDown => write!(f, "PageDown"),
            KeyCode::Home => write!(f, "Home"),
            KeyCode::End => write!(f, "End"),
            KeyCode::F(n) => write!(f, "F{n}"),
            _ => write!(f, "?"),
        }
    }
}

impl Serialize for KeyChord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Key resolution scope based on the active modal or screen context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyScope {
    /// General navigation and player controls.
    Global,
    /// Search overlay text entry.
    Search,
    /// Command palette search input.
    CommandPalette,
    /// Up Next queue drawer with reordering.
    Queue,
    /// Lyrics overlay.
    Lyrics,
}

/// Raw file schema for `keymap.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeymapConfigFile {
    #[serde(default)]
    pub bindings: HashMap<String, Action>,
    #[serde(default)]
    pub queue: HashMap<String, Action>,
}

/// Complete runtime keymap table.
#[derive(Debug, Clone)]
pub struct Keymap {
    global_bindings: Vec<(KeyChord, Action)>,
    queue_bindings: Vec<(KeyChord, Action)>,
}

impl Default for Keymap {
    fn default() -> Self {
        let global_bindings = vec![
            (KeyChord::new(KeyCode::Char('c')).ctrl(), Action::Quit),
            (KeyChord::new(KeyCode::Char('Q')), Action::Quit),
            (KeyChord::new(KeyCode::Esc), Action::Escape),
            (
                KeyChord::new(KeyCode::Char('k')).ctrl(),
                Action::ToggleCommandPalette,
            ),
            (KeyChord::new(KeyCode::Char('r')).ctrl(), Action::Refresh),
            (KeyChord::new(KeyCode::Char(' ')), Action::TogglePlay),
            (KeyChord::new(KeyCode::Char('q')), Action::ToggleQueue),
            (KeyChord::new(KeyCode::Char('l')), Action::ToggleLyrics),
            (KeyChord::new(KeyCode::Char('n')), Action::NextTrack),
            (KeyChord::new(KeyCode::Char('p')), Action::PrevTrack),
            (KeyChord::new(KeyCode::Char('+')), Action::VolumeUp),
            (KeyChord::new(KeyCode::Char('=')), Action::VolumeUp),
            (KeyChord::new(KeyCode::Char('-')), Action::VolumeDown),
            (KeyChord::new(KeyCode::Char('s')), Action::ToggleShuffle),
            (KeyChord::new(KeyCode::Char('r')), Action::CycleRepeat),
            (KeyChord::new(KeyCode::Char('/')), Action::OpenSearch),
            (KeyChord::new(KeyCode::Char('?')), Action::ToggleHelp),
            (KeyChord::new(KeyCode::Char(',')), Action::ToggleSettings),
            (KeyChord::new(KeyCode::Char('1')), Action::NavigateListenNow),
            (KeyChord::new(KeyCode::Char('2')), Action::NavigateBrowse),
            (KeyChord::new(KeyCode::Char('3')), Action::NavigateRadio),
            (KeyChord::new(KeyCode::Char('4')), Action::NavigateLibrary),
            (
                KeyChord::new(KeyCode::Char('5')),
                Action::NavigateNowPlaying,
            ),
        ];

        let queue_bindings = vec![
            (KeyChord::new(KeyCode::Up).alt(), Action::QueueMoveUp),
            (KeyChord::new(KeyCode::Down).alt(), Action::QueueMoveDown),
            (KeyChord::new(KeyCode::Delete), Action::QueueDelete),
            (KeyChord::new(KeyCode::Char('d')), Action::QueueDelete),
        ];

        Self {
            global_bindings,
            queue_bindings,
        }
    }
}

impl Keymap {
    /// Load keymap from the default configuration path `~/.config/malus/keymap.toml`.
    /// Falls back to default bindings if the file is missing or unreadable.
    pub fn load() -> Self {
        if let Some(proj_dirs) = ProjectDirs::from("com", "malus", "malus") {
            let path = proj_dirs.config_dir().join("keymap.toml");
            if let Ok(content) = fs::read_to_string(&path)
                && let Ok(keymap) = Self::load_from_str(&content)
            {
                return keymap;
            }
        }
        Self::default()
    }

    /// Load keymap from a specific file path.
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::load_from_str(&content)
    }

    /// Parse keymap from TOML string, merging custom bindings over defaults.
    pub fn load_from_str(toml_str: &str) -> Result<Self, String> {
        let config: KeymapConfigFile = toml::from_str(toml_str).map_err(|e| e.to_string())?;
        let mut keymap = Self::default();

        for (chord_str, action) in config.bindings {
            let chord = KeyChord::from_str(&chord_str)?;
            // Replace existing or prepend custom binding
            keymap.global_bindings.retain(|(c, _)| *c != chord);
            keymap.global_bindings.insert(0, (chord, action));
        }

        for (chord_str, action) in config.queue {
            let chord = KeyChord::from_str(&chord_str)?;
            keymap.queue_bindings.retain(|(c, _)| *c != chord);
            keymap.queue_bindings.insert(0, (chord, action));
        }

        Ok(keymap)
    }

    /// Resolve a key event in a given scope to an Action.
    pub fn resolve(&self, scope: KeyScope, key: &KeyEvent) -> Option<Action> {
        // 1. Check scope-specific bindings first
        if scope == KeyScope::Queue {
            for (chord, action) in &self.queue_bindings {
                if chord.matches(key) {
                    return Some(*action);
                }
            }
        }

        // 2. Special text-entry scopes intercept input before falling back
        if matches!(scope, KeyScope::Search | KeyScope::CommandPalette) {
            if key.modifiers.ctrl && key.code == KeyCode::Char('c') {
                return Some(Action::Quit);
            }
            if key.code == KeyCode::Esc {
                return Some(Action::Escape);
            }
            if key.modifiers.ctrl && key.code == KeyCode::Char('u') {
                return Some(Action::SearchClear);
            }
            if key.code == KeyCode::Backspace {
                return Some(if scope == KeyScope::Search {
                    Action::SearchBackspace
                } else {
                    Action::CommandBackspace
                });
            }
            // All other printable chars are consumed by text entry
            return None;
        }

        // 3. Check global bindings
        for (chord, action) in &self.global_bindings {
            if chord.matches(key) {
                return Some(*action);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratcn::runtime::Modifiers;

    #[test]
    fn test_chord_parsing_and_matching() {
        let chord = KeyChord::from_str("ctrl+c").unwrap();
        assert!(chord.ctrl);
        assert!(!chord.alt);
        assert_eq!(chord.code, KeyCode::Char('c'));

        let event = KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        };
        assert!(chord.matches(&event));

        let chord_space = KeyChord::from_str("space").unwrap();
        assert_eq!(chord_space.code, KeyCode::Char(' '));

        let chord_alt_down = KeyChord::from_str("alt+down").unwrap();
        assert!(chord_alt_down.alt);
        assert_eq!(chord_alt_down.code, KeyCode::Down);
    }

    #[test]
    fn test_default_keymap_resolves() {
        let keymap = Keymap::default();

        let space_event = KeyEvent {
            code: KeyCode::Char(' '),
            modifiers: Modifiers::NONE,
        };
        assert_eq!(
            keymap.resolve(KeyScope::Global, &space_event),
            Some(Action::TogglePlay)
        );

        let quit_event = KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        };
        assert_eq!(
            keymap.resolve(KeyScope::Global, &quit_event),
            Some(Action::Quit)
        );
        assert_eq!(
            keymap.resolve(KeyScope::Search, &quit_event),
            Some(Action::Quit)
        );
    }

    #[test]
    fn test_custom_keymap_override() {
        let toml_data = r#"
            [bindings]
            "x" = "toggle_play"
            "ctrl+q" = "quit"
        "#;
        let keymap = Keymap::load_from_str(toml_data).unwrap();

        let x_event = KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: Modifiers::NONE,
        };
        assert_eq!(
            keymap.resolve(KeyScope::Global, &x_event),
            Some(Action::TogglePlay)
        );
    }
}
