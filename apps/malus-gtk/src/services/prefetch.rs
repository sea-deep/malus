//! Client-side speculative page cache and prefetching service.
//!
//! Stores normalized PageWire responses in memory with a short TTL,
//! allowing instantaneous page switching without waiting for IPC or network round-trips.

use malus_client::MalusClient;
use malus_ipc::wire::PageWire;
use malus_model::PageRoute;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const PAGE_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes
const MAX_CACHE_ENTRIES: usize = 20;

#[derive(Clone)]
pub struct PageCache {
    entries: Arc<Mutex<HashMap<PageRoute, (Instant, PageWire)>>>,
    in_flight: Arc<Mutex<Vec<PageRoute>>>,
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Retrieves a cached page if present and within the TTL.
    pub async fn get(&self, route: &PageRoute) -> Option<PageWire> {
        let entries = self.entries.lock().await;
        if let Some((inserted_at, page)) = entries.get(route)
            && inserted_at.elapsed() < PAGE_CACHE_TTL
        {
            return Some(page.clone());
        }
        None
    }

    /// Synchronous non-blocking check for immediate rendering (e.g. from GTK thread).
    pub fn try_get(&self, route: &PageRoute) -> Option<PageWire> {
        if let Ok(entries) = self.entries.try_lock()
            && let Some((inserted_at, page)) = entries.get(route)
            && inserted_at.elapsed() < PAGE_CACHE_TTL
        {
            return Some(page.clone());
        }
        None
    }

    /// Stores a page in the cache.
    pub async fn insert(&self, route: PageRoute, page: PageWire) {
        let mut entries = self.entries.lock().await;
        if entries.len() >= MAX_CACHE_ENTRIES {
            // Evict oldest entry
            if let Some(oldest_key) = entries
                .iter()
                .min_by_key(|(_, (instant, _))| *instant)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest_key);
            }
        }
        entries.insert(route, (Instant::now(), page));
    }

    /// Synchronous non-blocking insert.
    pub fn try_insert(&self, route: PageRoute, page: PageWire) {
        if let Ok(mut entries) = self.entries.try_lock() {
            if entries.len() >= MAX_CACHE_ENTRIES
                && let Some(oldest_key) = entries
                    .iter()
                    .min_by_key(|(_, (instant, _))| *instant)
                    .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest_key);
            }
            entries.insert(route, (Instant::now(), page));
        }
    }

    /// Prefetches a specific route in the background if not already cached.
    pub fn prefetch(&self, client: MalusClient, route: PageRoute) {
        let cache = self.clone();
        tokio::spawn(async move {
            // Check if already in cache or in flight
            if cache.get(&route).await.is_some() {
                return;
            }
            {
                let mut in_flight = cache.in_flight.lock().await;
                if in_flight.contains(&route) {
                    return;
                }
                in_flight.push(route.clone());
            }

            if let Ok(page) = client.get_page(&route).await {
                cache.insert(route.clone(), page).await;
            }

            let mut in_flight = cache.in_flight.lock().await;
            in_flight.retain(|r| r != &route);
        });
    }

    /// Prefetches logical adjacent routes based on the user's current navigation.
    pub fn prefetch_related(&self, client: &MalusClient, current: &PageRoute) {
        match current {
            PageRoute::Home => {
                self.prefetch(client.clone(), PageRoute::New);
                self.prefetch(client.clone(), PageRoute::Radio);
            }
            PageRoute::LibraryRecentlyAdded => {
                self.prefetch(client.clone(), PageRoute::LibraryAlbums);
                self.prefetch(client.clone(), PageRoute::LibrarySongs);
            }
            PageRoute::LibraryAlbums => {
                self.prefetch(client.clone(), PageRoute::LibraryArtists);
                self.prefetch(client.clone(), PageRoute::LibraryPlaylists);
            }
            PageRoute::LibrarySongs => {
                self.prefetch(client.clone(), PageRoute::LibraryAlbums);
            }
            _ => {}
        }
    }
}
