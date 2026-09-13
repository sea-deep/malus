//! Canonical mathematical design tokens for Malus.
//!
//! Governed strictly by:
//! - 4px Base Grid
//! - 8px Primary Layout Rhythm
//! - 1.2 Modular Typography Progression
//! - Semantic GTK Symbolic Icon Theming

// --- Spacing Scale (4px Base Grid) ---
pub const SPACING_XXS: i32 = 4;
pub const SPACING_XS: i32 = 8;
pub const SPACING_SM: i32 = 12;
pub const SPACING_MD: i32 = 16;
pub const SPACING_LG: i32 = 24;
pub const SPACING_XL: i32 = 32;
pub const SPACING_XXL: i32 = 48;
pub const SPACING_XXXL: i32 = 64;

// --- Typography Scale (~1.2 Modular Scale) ---
pub const FONT_CAPTION: i32 = 13;
pub const FONT_BODY: i32 = 16;
pub const FONT_HEADING_SM: i32 = 19;
pub const FONT_HEADING_SECTION: i32 = 23;
pub const FONT_HEADING_PAGE: i32 = 32;

// Line Heights (px)
pub const LINE_HEIGHT_CAPTION: i32 = 18;
pub const LINE_HEIGHT_BODY: i32 = 24;
pub const LINE_HEIGHT_HEADING_SECTION: i32 = 28;
pub const LINE_HEIGHT_HEADING_PAGE: i32 = 40;

// --- Control & Interactive Sizes ---
pub const CONTROL_SM: i32 = 32;
pub const CONTROL_MD: i32 = 40;
pub const CONTROL_LG: i32 = 48;

// Icon Glyph Sizes
pub const ICON_GLYPH_SM: i32 = 16;
pub const ICON_GLYPH_MD: i32 = 20;
pub const ICON_GLYPH_LG: i32 = 24;

// --- Corner Radii Scale ---
pub const RADIUS_SM: i32 = 6;
pub const RADIUS_MD: i32 = 10;
pub const RADIUS_LG: i32 = 14;

// --- Sidebar Geometry ---
pub const SIDEBAR_WIDTH_NORMAL: f64 = 224.0;
pub const SIDEBAR_WIDTH_COMPACT: f64 = 192.0;
pub const SIDEBAR_WIDTH_SPACIOUS: f64 = 240.0;
pub const SIDEBAR_ROW_HEIGHT: i32 = 40;
pub const SIDEBAR_PADDING_HORIZONTAL: i32 = 12;
pub const SIDEBAR_SECTION_GAP: i32 = 24;

// --- Page & Content Grid Geometry ---
pub const PAGE_PADDING_NORMAL: i32 = 32;
pub const PAGE_PADDING_NARROW: i32 = 24;
pub const GRID_GAP: u32 = 24;
pub const ARTWORK_TILE_MIN_WIDTH: i32 = 160;
pub const ARTWORK_TILE_MAX_WIDTH: i32 = 220;

// --- Persistent Player Geometry ---
pub const PLAYER_TOTAL_HEIGHT: i32 = 88;
pub const PLAYER_CONTROLS_ROW_HEIGHT: i32 = 56;
pub const PLAYER_PROGRESS_ROW_HEIGHT: i32 = 32;
pub const PLAYER_ARTWORK_SIZE: i32 = 48;
pub const SEEK_TROUGH_THICKNESS: i32 = 4;
pub const SEEK_THUMB_IDLE: i32 = 8;
pub const SEEK_THUMB_HOVER: i32 = 12;
pub const TIME_LABEL_MIN_WIDTH: i32 = 44;

// --- GTK Semantic Symbolic Icons ---
pub const ICON_HOME: &str = "go-home-symbolic";
pub const ICON_SEARCH: &str = "system-search-symbolic";
pub const ICON_ALBUMS: &str = "media-optical-cd-audio-symbolic";
pub const ICON_SONGS: &str = "audio-x-generic-symbolic";
pub const ICON_PLAYLISTS: &str = "folder-music-symbolic";

pub const ICON_PLAY: &str = "media-playback-start-symbolic";
pub const ICON_PAUSE: &str = "media-playback-pause-symbolic";
pub const ICON_PREVIOUS: &str = "media-skip-backward-symbolic";
pub const ICON_NEXT: &str = "media-skip-forward-symbolic";
pub const ICON_SHUFFLE: &str = "media-playlist-shuffle-symbolic";
pub const ICON_REPEAT: &str = "media-playlist-repeat-symbolic";

pub const ICON_QUEUE: &str = "view-list-symbolic";
pub const ICON_VOLUME_HIGH: &str = "audio-volume-high-symbolic";
pub const ICON_VOLUME_MED: &str = "audio-volume-medium-symbolic";
pub const ICON_VOLUME_LOW: &str = "audio-volume-low-symbolic";
pub const ICON_VOLUME_MUTED: &str = "audio-volume-muted-symbolic";

pub const ICON_BACK: &str = "go-previous-symbolic";
pub const ICON_FORWARD: &str = "go-next-symbolic";
