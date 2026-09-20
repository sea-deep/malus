//! Recent searches persistence and model.

use malus_model::{MediaRef, PageRoute};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentSearchItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub artwork_url: Option<String>,
    pub is_circular: bool,
    pub is_library: bool,
    pub route: Option<PageRoute>,
    pub media_ref: Option<MediaRef>,
}

fn recent_searches_path() -> PathBuf {
    let mut path = relm4::gtk::glib::user_data_dir();
    path.push("malus");
    path.push("recent_searches.json");
    path
}

pub fn load_recent_searches() -> Vec<RecentSearchItem> {
    let path = recent_searches_path();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_recent_searches(items: &[RecentSearchItem]) {
    let path = recent_searches_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(items) {
        let _ = fs::write(&path, json);
    }
}

pub fn add_recent_search(item: RecentSearchItem) {
    let mut items = load_recent_searches();
    items.retain(|existing| existing.id != item.id);
    items.insert(0, item);
    items.truncate(20);
    save_recent_searches(&items);
}

pub fn clear_recent_searches() {
    let path = recent_searches_path();
    let _ = fs::remove_file(path);
}
