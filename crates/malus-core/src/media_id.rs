//! Strict provider-namespaced media identifiers.
//!
//! Enforces the opaque namespaced format: `<provider>:<kind>:<id>`
//! (e.g. `mock:track:3`, `apple:album:12345`, `spotify:artist:abc`).
//!
//! Core validates the general namespaced structure, but never interprets
//! provider-specific identifier contents.

use std::{fmt, ops::Deref, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaIdError {
    InvalidFormat(String),
    EmptyComponent(String),
    InvalidCharacter {
        component: &'static str,
        value: String,
    },
}

impl fmt::Display for MediaIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(s) => write!(
                f,
                "Invalid media ID '{s}': expected '<provider>:<kind>:<id>'"
            ),
            Self::EmptyComponent(s) => {
                write!(f, "Invalid media ID '{s}': components cannot be empty")
            }
            Self::InvalidCharacter { component, value } => write!(
                f,
                "Invalid media ID {component} '{value}': must be alphanumeric or contain '-' or '_'"
            ),
        }
    }
}

impl std::error::Error for MediaIdError {}

/// An opaque, namespaced media identifier (`<provider>:<kind>:<id>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaId {
    raw: String,
    provider_end: usize,
    kind_end: usize,
}

impl MediaId {
    /// Parse and validate a namespaced media identifier.
    ///
    /// Parses the first two colons `<provider>:<kind>:<opaque_id>`. The opaque
    /// identifier may contain arbitrary characters, including additional colons.
    pub fn parse(s: &str) -> Result<Self, MediaIdError> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 {
            return Err(MediaIdError::InvalidFormat(s.to_string()));
        }

        let (provider, kind, id) = (parts[0], parts[1], parts[2]);

        if provider.is_empty() || kind.is_empty() || id.is_empty() {
            return Err(MediaIdError::EmptyComponent(s.to_string()));
        }

        // Validate provider characters (alphanumeric, -, _)
        if !provider
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(MediaIdError::InvalidCharacter {
                component: "provider",
                value: provider.to_string(),
            });
        }

        // Validate kind characters (alphanumeric, -, _)
        if !kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(MediaIdError::InvalidCharacter {
                component: "kind",
                value: kind.to_string(),
            });
        }

        // Core does NOT validate or interpret provider-specific opaque `id` contents
        let provider_end = provider.len();
        let kind_end = provider_end + 1 + kind.len();

        Ok(Self {
            raw: s.to_string(),
            provider_end,
            kind_end,
        })
    }

    /// Construct a new `MediaId` from validated parts.
    pub fn new(provider: &str, kind: &str, id: &str) -> Result<Self, MediaIdError> {
        let raw = format!("{provider}:{kind}:{id}");
        Self::parse(&raw)
    }

    /// The provider namespace (e.g. "mock", "apple", "spotify").
    pub fn provider(&self) -> &str {
        &self.raw[..self.provider_end]
    }

    /// The resource kind (e.g. "track", "album", "artist", "playlist").
    pub fn kind(&self) -> &str {
        &self.raw[self.provider_end + 1..self.kind_end]
    }

    /// The provider-specific opaque item identifier.
    pub fn id(&self) -> &str {
        &self.raw[self.kind_end + 1..]
    }

    /// The provider-specific opaque item identifier (alias for `id()`).
    pub fn opaque(&self) -> &str {
        self.id()
    }

    /// Get the full string representation.
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl Deref for MediaId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl fmt::Display for MediaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl FromStr for MediaId {
    type Err = MediaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for MediaId {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_media_ids() {
        let valid_cases = [
            ("mock:track:3", "mock", "track", "3"),
            ("mock:album:2", "mock", "album", "2"),
            ("mock:artist:1", "mock", "artist", "1"),
            ("mock:playlist:4", "mock", "playlist", "4"),
            (
                "apple_music:track:1234567890",
                "apple_music",
                "track",
                "1234567890",
            ),
            (
                "spotify-us:album:6rqhFgbbKwnb9MLmUQDhG6",
                "spotify-us",
                "album",
                "6rqhFgbbKwnb9MLmUQDhG6",
            ),
            (
                "custom:track:raw/path?with=symbols&more_underscores",
                "custom",
                "track",
                "raw/path?with=symbols&more_underscores",
            ),
            (
                "foo:track:some:weird:native:id",
                "foo",
                "track",
                "some:weird:native:id",
            ),
            (
                "spotify:track:abc:def:123",
                "spotify",
                "track",
                "abc:def:123",
            ),
        ];

        for (raw, expected_prov, expected_kind, expected_id) in valid_cases {
            let id = MediaId::parse(raw)
                .unwrap_or_else(|e| panic!("Failed to parse valid ID '{raw}': {e}"));
            assert_eq!(id.provider(), expected_prov);
            assert_eq!(id.kind(), expected_kind);
            assert_eq!(id.id(), expected_id);
            assert_eq!(id.opaque(), expected_id);
            assert_eq!(id.as_str(), raw);
            assert_eq!(id.to_string(), raw);
        }
    }

    #[test]
    fn test_invalid_media_ids() {
        let invalid_cases = [
            "mock-3",             // Missing colons
            "3",                  // No namespace
            "",                   // Empty
            "mock:3",             // Only two parts
            ":track:3",           // Empty provider
            "mock::3",            // Empty kind
            "mock:track:",        // Empty id
            "mock space:track:1", // Invalid character in provider
            "mock:track kind:1",  // Invalid character in kind
        ];

        for raw in invalid_cases {
            assert!(
                MediaId::parse(raw).is_err(),
                "Expected '{raw}' to be rejected as invalid MediaId"
            );
        }
    }
}
