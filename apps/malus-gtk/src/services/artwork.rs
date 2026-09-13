//! Shared application-scoped artwork service and provider-neutral decoding pipeline.
//!
//! Enforces:
//! - Bounded network concurrency (~6)
//! - In-flight URL deduplication (fan-out to all concurrent requesters)
//! - Bounded decoded-image LRU in memory (~128 entries)
//! - Optional on-disk byte cache ($XDG_CACHE_HOME/malus/artwork)
//! - Strict resource bounds: 8MB max response, 4096x4096 max dimensions, 10s timeout
//! - Pure provider-neutral decoded RGBA bytes (GDK Texture created exclusively on GTK main thread)

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore, broadcast};

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub rgba: Vec<u8>,
}

const MAX_IMAGE_RESPONSE_BYTES: usize = 8 * 1024 * 1024; // 8 MB
const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 4096 * 4096;
const DEFAULT_LRU_CAPACITY: usize = 128;
const MAX_CONCURRENT_FETCHES: usize = 6;

type InFlightMap = HashMap<String, broadcast::Sender<Option<Arc<DecodedImage>>>>;

struct MemoryLru {
    capacity: usize,
    entries: HashMap<String, Arc<DecodedImage>>,
    order: VecDeque<String>,
}

impl MemoryLru {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, url: &str) -> Option<Arc<DecodedImage>> {
        if let Some(img) = self.entries.get(url) {
            let img = img.clone();
            self.order.retain(|k| k != url);
            self.order.push_back(url.to_string());
            Some(img)
        } else {
            None
        }
    }

    fn insert(&mut self, url: String, img: Arc<DecodedImage>) {
        if self.entries.contains_key(&url) {
            self.entries.insert(url.clone(), img);
            self.order.retain(|k| k != &url);
            self.order.push_back(url);
            return;
        }

        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }

        self.order.push_back(url.clone());
        self.entries.insert(url, img);
    }
}

#[derive(Clone)]
pub struct ArtworkService {
    lru: Arc<Mutex<MemoryLru>>,
    in_flight: Arc<Mutex<InFlightMap>>,
    semaphore: Arc<Semaphore>,
    cache_dir: Option<PathBuf>,
    http_client: reqwest::Client,
}

impl fmt::Debug for ArtworkService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtworkService")
            .field("cache_dir", &self.cache_dir)
            .finish()
    }
}

impl Default for ArtworkService {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtworkService {
    pub fn new() -> Self {
        let cache_dir = dirs_fallback().map(|base| base.join("malus").join("artwork"));
        if let Some(ref dir) = cache_dir {
            let _ = std::fs::create_dir_all(dir);
        }

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            lru: Arc::new(Mutex::new(MemoryLru::new(DEFAULT_LRU_CAPACITY))),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
            cache_dir,
            http_client,
        }
    }

    pub async fn load(&self, url: &str) -> Option<Arc<DecodedImage>> {
        if url.trim().is_empty() {
            return None;
        }

        // 1. Check in-memory LRU cache
        {
            let mut lru = self.lru.lock().await;
            if let Some(cached) = lru.get(url) {
                return Some(cached);
            }
        }

        // 2. Check in-flight requests (deduplication)
        let mut rx = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(tx) = in_flight.get(url) {
                Some(tx.subscribe())
            } else {
                let (tx, _) = broadcast::channel(1);
                in_flight.insert(url.to_string(), tx);
                None
            }
        };

        if let Some(ref mut subscriber) = rx {
            if let Ok(result) = subscriber.recv().await {
                return result;
            }
            return None;
        }

        // 3. Leader fetcher path
        let result = self.fetch_and_decode(url).await;

        // Broadcast to followers and remove from in_flight
        {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(tx) = in_flight.remove(url) {
                let _ = tx.send(result.clone());
            }
        }

        // Cache successful image in LRU
        if let Some(ref img) = result {
            let mut lru = self.lru.lock().await;
            lru.insert(url.to_string(), img.clone());
        }

        result
    }

    async fn fetch_and_decode(&self, url: &str) -> Option<Arc<DecodedImage>> {
        let _permit = self.semaphore.acquire().await.ok()?;

        // Check disk cache
        if let Some(ref dir) = self.cache_dir {
            let filename = format!("{:016x}", fnv1a_hash(url.as_bytes()));
            let path = dir.join(filename);
            if let Ok(bytes) = tokio::fs::read(&path).await {
                if let Some(decoded) = decode_image_bytes(bytes).await {
                    return Some(Arc::new(decoded));
                }
            }
        }

        // Network fetch
        let resp = self.http_client.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }

        let bytes = resp.bytes().await.ok()?;
        if bytes.len() > MAX_IMAGE_RESPONSE_BYTES {
            return None;
        }

        // Write to disk cache
        if let Some(ref dir) = self.cache_dir {
            let filename = format!("{:016x}", fnv1a_hash(url.as_bytes()));
            let path = dir.join(filename);
            let _ = tokio::fs::write(&path, &bytes).await;
        }

        let decoded = decode_image_bytes(bytes.to_vec()).await?;
        Some(Arc::new(decoded))
    }
}

async fn decode_image_bytes(bytes: Vec<u8>) -> Option<DecodedImage> {
    tokio::task::spawn_blocking(move || {
        let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .ok()?;
        let (width, height) = reader.into_dimensions().ok()?;

        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return None;
        }
        if (width as u64) * (height as u64) > MAX_PIXELS {
            return None;
        }

        let img = image::load_from_memory(&bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        if w > MAX_DIMENSION || h > MAX_DIMENSION {
            return None;
        }

        let stride = (w * 4) as usize;
        Some(DecodedImage {
            width: w,
            height: h,
            stride,
            rgba: rgba.into_raw(),
        })
    })
    .await
    .ok()?
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn dirs_fallback() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("XDG_CACHE_HOME")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".cache"));
    }
    None
}
