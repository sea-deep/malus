//! Pure domain song credits model.

use serde::{Deserialize, Serialize};

/// Categorized credits for a track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Credits {
    pub categories: Vec<CreditCategory>,
}

impl Credits {
    pub fn new(categories: Vec<CreditCategory>) -> Self {
        Self { categories }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }
}

/// A category of credits (e.g. Performing Artists, Production & Engineering, Composition & Lyrics).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditCategory {
    pub title: String,
    pub kind: String,
    pub items: Vec<CreditItem>,
}

impl CreditCategory {
    pub fn new(title: impl Into<String>, kind: impl Into<String>, items: Vec<CreditItem>) -> Self {
        Self {
            title: title.into(),
            kind: kind.into(),
            items,
        }
    }
}

/// An individual credited person/entity with their roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditItem {
    pub name: String,
    pub roles: Vec<String>,
}

impl CreditItem {
    pub fn new(name: impl Into<String>, roles: Vec<String>) -> Self {
        Self {
            name: name.into(),
            roles,
        }
    }
}
