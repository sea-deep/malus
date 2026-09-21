//! Shared application-scoped artwork service and provider-neutral decoding pipeline.
//!
//! Enforces:
//! - Bounded network concurrency (~6)
//! - In-flight URL deduplication (fan-out to all concurrent requesters)
//! - Bounded decoded-image LRU in memory (~32 entries)
//! - Target-size aware network requests & thumbnail decoding (small/medium/large buckets)
//! - Zero-copy GBytes transfer to GTK MemoryTexture
//! - Optional on-disk byte cache ($XDG_CACHE_HOME/malus/artwork)
//! - On-demand backdrop generation for Now Playing (no wasted blur calculations on feed items)
//! - Strict resource bounds: 8MB max response, 4096x4096 max dimensions, 10s timeout

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
    pub bytes: relm4::gtk::glib::Bytes,
}

impl DecodedImage {
    #[inline]
    pub fn as_bytes(&self) -> &relm4::gtk::glib::Bytes {
        &self.bytes
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

const MAX_IMAGE_RESPONSE_BYTES: usize = 8 * 1024 * 1024; // 8 MB
const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 4096 * 4096;
const MAX_CACHE_BYTES: usize = 32 * 1024 * 1024; // 32 MB budget for decoded image cache
const MAX_CONCURRENT_FETCHES: usize = 4;

pub const THUMB_SMALL: u32 = 160;
pub const THUMB_MEDIUM: u32 = 480;
pub const THUMB_LARGE: u32 = 800;
pub const THUMB_HERO: u32 = 1200;

pub fn normalize_target_size(size: u32) -> u32 {
    if size <= 160 {
        THUMB_SMALL
    } else if size <= 320 {
        THUMB_MEDIUM
    } else if size <= 800 {
        THUMB_LARGE
    } else {
        THUMB_HERO
    }
}

pub fn resolve_artwork_url(url: &str, target_size: u32) -> String {
    let bucket = normalize_target_size(target_size);
    if url.contains("{w}") || url.contains("{h}") {
        return url
            .replace("{w}", &bucket.to_string())
            .replace("{h}", &bucket.to_string())
            .replace("{c}", "bb")
            .replace("{f}", "jpg");
    }
    if url.contains(".mzstatic.com/")
        && let Some(rewritten) = rewrite_mzstatic_dimensions(url, bucket)
    {
        return rewritten;
    }
    url.to_string()
}

fn rewrite_mzstatic_dimensions(url: &str, bucket: u32) -> Option<String> {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (url, None),
    };

    let last_slash_idx = base.rfind('/')?;
    let prefix = &base[..last_slash_idx];
    let filename = &base[last_slash_idx + 1..];

    let x_pos = filename.find('x')?;
    let w_str = &filename[..x_pos];
    if w_str.is_empty() || !w_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let rest = &filename[x_pos + 1..];
    let h_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let h_str = &rest[..h_end];
    if h_str.is_empty() {
        return None;
    }

    let after_h = &rest[h_end..];
    let (suffix, ext) = if let Some(dot_pos) = after_h.rfind('.') {
        (&after_h[..dot_pos], &after_h[dot_pos + 1..])
    } else {
        ("", "jpg")
    };

    let crop_flag = if suffix.is_empty() { "bb" } else { suffix };

    let ext = if ext.is_empty() { "jpg" } else { ext };
    let new_filename = format!("{bucket}x{bucket}{crop_flag}.{ext}");

    let mut result = format!("{prefix}/{new_filename}");
    if let Some(q) = query {
        result.push('?');
        result.push_str(q);
    }
    Some(result)
}

type InFlightMap = HashMap<String, broadcast::Sender<Option<relm4::gtk::glib::Bytes>>>;

struct CacheEntry {
    bytes: relm4::gtk::glib::Bytes,
    len: usize,
}

struct MemoryLru {
    max_bytes: usize,
    current_bytes: usize,
    entries: HashMap<String, CacheEntry>,
    order: VecDeque<String>,
}

impl MemoryLru {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            current_bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<relm4::gtk::glib::Bytes> {
        if let Some(entry) = self.entries.get(key) {
            let bytes = entry.bytes.clone();
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_string());
            Some(bytes)
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, bytes: relm4::gtk::glib::Bytes) {
        let len = bytes.len();
        if let Some(existing) = self.entries.insert(key.clone(), CacheEntry { bytes, len }) {
            self.current_bytes = self.current_bytes.saturating_sub(existing.len);
            self.current_bytes = self.current_bytes.saturating_add(len);
            self.order.retain(|k| k != &key);
            self.order.push_back(key);
        } else {
            self.current_bytes = self.current_bytes.saturating_add(len);
            self.order.push_back(key);
        }

        while self.current_bytes > self.max_bytes && !self.order.is_empty() {
            if let Some(oldest) = self.order.pop_front()
                && let Some(removed) = self.entries.remove(&oldest)
            {
                self.current_bytes = self.current_bytes.saturating_sub(removed.len);
            }
        }
    }
}

#[derive(Clone)]
pub struct ArtworkService {
    lru: Arc<Mutex<MemoryLru>>,
    in_flight: Arc<Mutex<InFlightMap>>,
    fetch_semaphore: Arc<Semaphore>,
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

fn spawn_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
    } else {
        relm4::spawn(future);
    }
}

impl ArtworkService {
    pub fn new() -> Self {
        let cache_dir = dirs_fallback().map(|base| base.join("malus").join("artwork"));
        if let Some(ref dir) = cache_dir {
            let _ = std::fs::create_dir_all(dir);
            let cleanup_dir = dir.clone();
            spawn_task(async move {
                if let Ok(mut entries) = tokio::fs::read_dir(cleanup_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if !name_str.starts_with("v3_") {
                            let _ = tokio::fs::remove_file(entry.path()).await;
                        }
                    }
                }
            });
        }

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            lru: Arc::new(Mutex::new(MemoryLru::new(MAX_CACHE_BYTES))),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            fetch_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES)),
            cache_dir,
            http_client,
        }
    }

    pub async fn load(&self, url: &str) -> Option<Arc<DecodedImage>> {
        self.load_sized(url, THUMB_LARGE).await
    }

    pub async fn load_bytes_sized(
        &self,
        url: &str,
        target_size: u32,
    ) -> Option<relm4::gtk::glib::Bytes> {
        if url.trim().is_empty() {
            return None;
        }

        let bucket = normalize_target_size(target_size);
        let resolved_url = resolve_artwork_url(url, bucket);
        let cache_key = if resolved_url == url && bucket != THUMB_LARGE {
            format!("{url}@{bucket}")
        } else {
            resolved_url.clone()
        };

        // 1. Check in-memory LRU cache
        {
            let mut lru = self.lru.lock().await;
            if let Some(cached) = lru.get(&cache_key) {
                return Some(cached);
            }
        }

        // 2. In-flight deduplication
        let mut receiver = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(tx) = in_flight.get(&cache_key) {
                tx.subscribe()
            } else {
                if let Some(cached) = self.lru.lock().await.get(&cache_key) {
                    return Some(cached);
                }
                let (tx, receiver) = broadcast::channel(1);
                in_flight.insert(cache_key.clone(), tx);
                let service = self.clone();
                let original_url = url.to_string();
                let fetch_url = resolved_url;
                let key = cache_key;
                tokio::spawn(async move {
                    let result = service.fetch_raw_bytes(&original_url, &fetch_url).await;
                    let mut in_flight = service.in_flight.lock().await;
                    if let Some(ref bytes) = result {
                        service.lru.lock().await.insert(key.clone(), bytes.clone());
                    }
                    if let Some(tx) = in_flight.remove(&key) {
                        let _ = tx.send(result);
                    }
                });
                receiver
            }
        };
        receiver.recv().await.ok().flatten()
    }

    pub async fn load_sized(&self, url: &str, target_size: u32) -> Option<Arc<DecodedImage>> {
        let bytes = self.load_bytes_sized(url, target_size).await?;
        let bucket = normalize_target_size(target_size);
        let decoded = decode_image_bytes(bytes.as_ref().to_vec(), bucket).await?;
        Some(Arc::new(decoded))
    }

    /// Asynchronously prefetches and caches raw artwork bytes without decoding.
    pub fn prefetch(&self, url: &str, target_size: u32) {
        let service = self.clone();
        let url = url.to_string();
        spawn_task(async move {
            let _ = service.load_bytes_sized(&url, target_size).await;
        });
    }

    /// Batch prefetches artwork items in the background.
    pub fn prefetch_batch(&self, urls: impl IntoIterator<Item = (String, u32)>) {
        for (url, size) in urls {
            self.prefetch(&url, size);
        }
    }

    async fn fetch_raw_bytes(
        &self,
        original_url: &str,
        fetch_url: &str,
    ) -> Option<relm4::gtk::glib::Bytes> {
        let _permit = tokio::time::timeout(Duration::from_secs(15), self.fetch_semaphore.acquire())
            .await
            .ok()?
            .ok()?;

        // Check disk cache for the sized file
        if let Some(ref dir) = self.cache_dir {
            let sized_filename = format!("v3_{:016x}", fnv1a_hash(fetch_url.as_bytes()));
            let sized_path = dir.join(sized_filename);
            if let Ok(metadata) = tokio::fs::metadata(&sized_path).await
                && metadata.len() <= MAX_IMAGE_RESPONSE_BYTES as u64
                && let Ok(bytes) = tokio::fs::read(&sized_path).await
            {
                return Some(relm4::gtk::glib::Bytes::from_owned(bytes));
            }
        }

        // Network fetch: fetch the sized URL
        let mut resp = self.http_client.get(fetch_url).send().await.ok()?;
        if !resp.status().is_success() {
            if fetch_url != original_url {
                resp = self.http_client.get(original_url).send().await.ok()?;
                if !resp.status().is_success() {
                    return None;
                }
            } else {
                return None;
            }
        }

        if resp
            .content_length()
            .is_some_and(|size| size > MAX_IMAGE_RESPONSE_BYTES as u64)
        {
            return None;
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.ok()? {
            if bytes.len().saturating_add(chunk.len()) > MAX_IMAGE_RESPONSE_BYTES {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }

        // Write to disk cache
        if let Some(ref dir) = self.cache_dir {
            let filename = format!("v3_{:016x}", fnv1a_hash(fetch_url.as_bytes()));
            let path = dir.join(filename);
            let _ = tokio::fs::write(&path, &bytes).await;
        }

        Some(relm4::gtk::glib::Bytes::from_owned(bytes))
    }
}

struct DecodeRequest {
    bytes: Vec<u8>,
    bucket: u32,
    respond_to: tokio::sync::oneshot::Sender<Option<DecodedImage>>,
}

static DECODE_QUEUE: std::sync::LazyLock<tokio::sync::mpsc::Sender<DecodeRequest>> =
    std::sync::LazyLock::new(|| {
        let (tx, rx) = tokio::sync::mpsc::channel::<DecodeRequest>(32);
        let rx = Arc::new(std::sync::Mutex::new(rx));
        for i in 0..2 {
            let rx = rx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("malus-dec-{i}"))
                .spawn(move || {
                    loop {
                        let req = {
                            let mut lock = match rx.lock() {
                                Ok(l) => l,
                                Err(_) => break,
                            };
                            lock.blocking_recv()
                        };
                        match req {
                            Some(req) => {
                                let res = decode_image_sync(req.bytes, req.bucket);
                                let _ = req.respond_to.send(res);
                            }
                            None => break,
                        }
                    }
                });
        }
        tx
    });

async fn decode_image_bytes(bytes: Vec<u8>, max_dimension: u32) -> Option<DecodedImage> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    DECODE_QUEUE
        .send(DecodeRequest {
            bytes,
            bucket: max_dimension,
            respond_to: tx,
        })
        .await
        .ok()?;
    rx.await.ok().flatten()
}

fn decode_image_sync(bytes: Vec<u8>, max_dimension: u32) -> Option<DecodedImage> {
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
    drop(bytes);
    let target = max_dimension.clamp(16, 1200);
    let rgba = if width > target || height > target {
        let resized = img.resize(target, target, image::imageops::FilterType::CatmullRom);
        drop(img);
        resized.into_rgba8()
    } else {
        img.into_rgba8()
    };

    let (w, h) = rgba.dimensions();
    let stride = (w * 4) as usize;
    let glib_bytes = relm4::gtk::glib::Bytes::from_owned(rgba.into_raw());

    Some(DecodedImage {
        width: w,
        height: h,
        stride,
        bytes: glib_bytes,
    })
}

/// Derives a softened, darkened 96px backdrop texture from a decoded image.
/// Computed on-demand exclusively for Now Playing without intermediate full-sized clones.
pub fn generate_backdrop_texture(image: &DecodedImage) -> Option<relm4::gtk::gdk::MemoryTexture> {
    use relm4::gtk::{gdk, glib};

    if image.width == 0 || image.height == 0 {
        return None;
    }

    let mut backdrop = image::RgbaImage::new(96, 96);
    let x_scale = image.width as f32 / 96.0;
    let y_scale = image.height as f32 / 96.0;
    let raw = image.bytes.as_ref();

    for y in 0..96 {
        let src_y = ((y as f32 + 0.5) * y_scale) as usize;
        let row_offset = src_y * image.stride;
        for x in 0..96 {
            let src_x = ((x as f32 + 0.5) * x_scale) as usize;
            let pixel_offset = row_offset + src_x * 4;
            if pixel_offset + 3 < raw.len() {
                let r = raw[pixel_offset];
                let g = raw[pixel_offset + 1];
                let b = raw[pixel_offset + 2];
                backdrop.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
    }

    let mut blurred = image::DynamicImage::ImageRgba8(backdrop)
        .blur(6.0)
        .into_rgba8();

    for pixel in blurred.pixels_mut() {
        let luma = 0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32;
        for c in 0..3 {
            pixel[c] = ((pixel[c] as f32 * 0.84 + luma * 0.16) * 0.52).round() as u8;
        }
        pixel[3] = 255;
    }
    let bytes = glib::Bytes::from_owned(blurred.into_raw());
    Some(gdk::MemoryTexture::new(
        96,
        96,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        96 * 4,
    ))
}

static BIND_URL_QUARK: std::sync::LazyLock<relm4::gtk::glib::Quark> =
    std::sync::LazyLock::new(|| relm4::gtk::glib::Quark::from_str("malus-bound-artwork-url"));

/// Asynchronously binds an artwork URL to a `gtk::Picture` without any component overhead.
/// Discards stale completion if the picture was rebound in the meantime.
pub fn bind_artwork(
    picture: &relm4::gtk::Picture,
    service: &ArtworkService,
    url: Option<String>,
    target_size: u32,
) {
    if let Some(url) = url {
        if url.trim().is_empty() {
            return;
        }
        use relm4::gtk::prelude::*;
        unsafe {
            picture.set_qdata(*BIND_URL_QUARK, url.clone());
        }
        let pic = picture.downgrade();
        let svc = service.clone();
        let requested_url = url.clone();
        let fetch = relm4::spawn(async move { svc.load_sized(&requested_url, target_size).await });
        relm4::gtk::glib::spawn_future_local(async move {
            if let Ok(Some(img)) = fetch.await
                && let Some(pic) = pic.upgrade()
            {
                let still_matches = unsafe {
                    pic.qdata::<String>(*BIND_URL_QUARK)
                        .map(|ptr| ptr.as_ref() == &url)
                        .unwrap_or(false)
                };
                if still_matches {
                    let texture = relm4::gtk::gdk::MemoryTexture::new(
                        img.width as i32,
                        img.height as i32,
                        relm4::gtk::gdk::MemoryFormat::R8g8b8a8,
                        &img.bytes,
                        img.stride,
                    );
                    pic.set_paintable(Some(&texture));
                }
            }
        });
    }
}

/// Upload a decoded image on the GTK thread; expensive work is already cached.
pub fn artwork_texture(image: &DecodedImage) -> relm4::gtk::gdk::MemoryTexture {
    use relm4::gtk::gdk;
    gdk::MemoryTexture::new(
        image.width as i32,
        image.height as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &image.bytes,
        image.stride,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_target_size() {
        assert_eq!(normalize_target_size(64), THUMB_SMALL);
        assert_eq!(normalize_target_size(160), THUMB_SMALL);
        assert_eq!(normalize_target_size(200), THUMB_MEDIUM);
        assert_eq!(normalize_target_size(320), THUMB_MEDIUM);
        assert_eq!(normalize_target_size(464), THUMB_LARGE);
        assert_eq!(normalize_target_size(800), THUMB_LARGE);
        assert_eq!(normalize_target_size(1200), THUMB_HERO);
        assert_eq!(normalize_target_size(1400), THUMB_HERO);
    }

    #[test]
    fn test_resolve_artwork_url_template() {
        let templ = "https://is1-ssl.mzstatic.com/image/thumb/Features/v4/source/{w}x{h}{c}.{f}";
        assert_eq!(
            resolve_artwork_url(templ, 464),
            "https://is1-ssl.mzstatic.com/image/thumb/Features/v4/source/800x800bb.jpg"
        );
        assert_eq!(
            resolve_artwork_url(templ, 128),
            "https://is1-ssl.mzstatic.com/image/thumb/Features/v4/source/160x160bb.jpg"
        );
    }

    #[test]
    fn test_resolve_artwork_url_fixed_dimensions() {
        // Standard 600x600bb
        let u1 = "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/e8/43/5f/e8435ffa-b6b9-b171-40ab-4ff3959ab661/886443919266.jpg/600x600bb.jpg";
        assert_eq!(
            resolve_artwork_url(u1, 464),
            "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/e8/43/5f/e8435ffa-b6b9-b171-40ab-4ff3959ab661/886443919266.jpg/800x800bb.jpg"
        );

        // Special suffix with query param (e.g. Pop Chill on New page)
        let u2 = "https://is1-ssl.mzstatic.com/image/thumb/Features122/v4/b4/e4/3e/b4e43e28-d2cb-2169-0106-6b9c80163b34/e581e29d-a3f4-40c5-9ec4-8c747b5fc7dc.png/600x600SC.DN01.jpg?l=en-GB";
        assert_eq!(
            resolve_artwork_url(u2, 464),
            "https://is1-ssl.mzstatic.com/image/thumb/Features122/v4/b4/e4/3e/b4e43e28-d2cb-2169-0106-6b9c80163b34/e581e29d-a3f4-40c5-9ec4-8c747b5fc7dc.png/800x800SC.DN01.jpg?l=en-GB"
        );

        // Smart-rectangle crop (e.g. Daily Top 100 / City Charts / Radio stations)
        let u3 = "https://is1-ssl.mzstatic.com/image/thumb/Features116/v4/c6/05/b6/c605b65a-427f-0165-33c1-a8d2f99877e9/5f02f9bb-8eb2-4c79-bdb8-6bfb24a57e96.png/1200x300sr.jpg";
        assert_eq!(
            resolve_artwork_url(u3, 464),
            "https://is1-ssl.mzstatic.com/image/thumb/Features116/v4/c6/05/b6/c605b65a-427f-0165-33c1-a8d2f99877e9/5f02f9bb-8eb2-4c79-bdb8-6bfb24a57e96.png/800x800sr.jpg"
        );

        // Smart playlist canvas crop (e.g. "Playlists Made for You": Your Essentials, Chill, Get Up!)
        let u4 = "https://is1-ssl.mzstatic.com/image/thumb/Features/v4/94/ba/6e/94ba6ea4-bdf0-476e-669b-b2613ae8fd0c/03c6932b-81b6-404e-9b1e-ffc2bda36f9e.png/1200x300cc.jpg";
        assert_eq!(
            resolve_artwork_url(u4, 464),
            "https://is1-ssl.mzstatic.com/image/thumb/Features/v4/94/ba/6e/94ba6ea4-bdf0-476e-669b-b2613ae8fd0c/03c6932b-81b6-404e-9b1e-ffc2bda36f9e.png/800x800cc.jpg"
        );

        // Non-Apple URL left intact
        let non_apple = "https://example.com/artwork.jpg";
        assert_eq!(resolve_artwork_url(non_apple, 464), non_apple);
    }
}
