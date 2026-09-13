//! Pure domain account media state model.

use serde::{Deserialize, Serialize};

use crate::MediaRef;

/// Apple Music rating state for a media resource (independent from favorite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    #[default]
    Neutral,
    SuggestLess,
}

impl Rating {
    pub fn is_suggest_less(&self) -> bool {
        matches!(self, Self::SuggestLess)
    }

    pub fn is_neutral(&self) -> bool {
        matches!(self, Self::Neutral)
    }
}

/// State of a media item in the authenticated user's account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMediaState {
    pub reference: MediaRef,
    pub in_library: bool,
    pub favorite: bool,
    pub rating: Rating,
}

impl AccountMediaState {
    pub fn new(reference: MediaRef, in_library: bool, favorite: bool, rating: Rating) -> Self {
        Self {
            reference,
            in_library,
            favorite,
            rating,
        }
    }

    pub fn is_favorite(&self) -> bool {
        self.favorite
    }

    pub fn is_suggest_less(&self) -> bool {
        self.rating.is_suggest_less()
    }

    pub fn in_library(&self) -> bool {
        self.in_library
    }
}
