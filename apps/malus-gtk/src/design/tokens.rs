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
pub const FONT_BODY: i32 = 15;
pub const FONT_HEADING_SM: i32 = 18;
pub const FONT_HEADING_SECTION: i32 = 22;
pub const FONT_HEADING_PAGE: i32 = 32;

// Line Heights (px)
pub const LINE_HEIGHT_CAPTION: i32 = 18;
pub const LINE_HEIGHT_BODY: i32 = 22;
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
pub const ICON_BRAND_SIDEBAR: i32 = 28;

// --- Corner Radii Scale ---
pub const RADIUS_SM: i32 = 6;
pub const RADIUS_MD: i32 = 8;
pub const RADIUS_LG: i32 = 10;
pub const RADIUS_XL: i32 = 12;

// --- Sidebar Geometry ---
pub const SIDEBAR_WIDTH_NORMAL: f64 = 232.0;
pub const SIDEBAR_WIDTH_COMPACT: f64 = 192.0;
pub const SIDEBAR_ROW_HEIGHT: i32 = 32;
pub const SIDEBAR_PADDING_HORIZONTAL: i32 = 10;
pub const SIDEBAR_SECTION_GAP: i32 = 14;

// --- Right Utility Pane Geometry ---
pub const UTILITY_PANE_WIDTH: f64 = 340.0;
pub const UTILITY_PANE_MIN_WIDTH: f64 = 280.0;
pub const UTILITY_PANE_MAX_WIDTH: f64 = 420.0;

// --- Responsive Layout Breakpoints ---
pub const BREAKPOINT_COLLAPSE_NAVIGATION: f64 = 860.0;

// --- Page & Content Grid Geometry ---
pub const PAGE_PADDING_NORMAL: i32 = 32;
pub const PAGE_PADDING_NARROW: i32 = 16;
pub const GRID_GAP: u32 = 20;
pub const ARTWORK_GRID_SIZE: i32 = 160;
pub const ARTWORK_SHELF_SIZE: i32 = 176;
pub const ARTWORK_TOP_PICKS_SIZE: i32 = 232;
pub const ARTWORK_HERO_SIZE: i32 = 240;
pub const BANNER_HERO_HEIGHT: i32 = 280;
pub const ARTWORK_LATEST_RELEASE_SIZE: i32 = 112;
pub const ARTWORK_SEARCH_LOCKUP_SIZE: i32 = 64;

// --- Rich Layout Components (New & Radio) ---
pub const LIVE_STATION_PILL_WIDTH: i32 = 208;
pub const LIVE_STATION_PILL_HEIGHT: i32 = 64;
pub const STATION_CARD_SIZE: i32 = 168;
pub const MULTIROW_TRACK_THUMB_SIZE: i32 = 44;
pub const MULTIROW_TRACK_WIDTH: i32 = 296;
pub const MULTIROW_EPISODE_THUMB_SIZE: i32 = 56;
pub const MULTIROW_EPISODE_WIDTH: i32 = 320;
pub const FEATURED_BANNER_WIDTH: i32 = 360;
pub const FEATURED_BANNER_HEIGHT: i32 = 200;

// --- Search Components ---
pub const CATEGORY_CARD_WIDTH: i32 = 224;
pub const CATEGORY_CARD_HEIGHT: i32 = 126;
pub const RECENT_SEARCH_THUMB_SIZE: i32 = 48;
pub const RECENT_SEARCH_CARD_WIDTH: i32 = 260;
pub const SEARCH_BAR_WIDTH_MIN: i32 = 200;
pub const SEARCH_BAR_WIDTH_MAX: i32 = 520;

// --- Persistent Player Geometry ---
pub const PLAYER_TOTAL_HEIGHT: i32 = 84;
pub const PLAYER_ARTWORK_SIZE: i32 = 56;
pub const SEEK_TROUGH_THICKNESS: i32 = 4;
pub const SEEK_TROUGH_THICKNESS_ACTIVE: i32 = 6;
pub const SEEK_THUMB_SIZE: i32 = 16;
pub const SEEK_THUMB_MARGIN: i32 = -4;
pub const VOLUME_POPOVER_SLIDER_WIDTH: i32 = 110;
pub const VOLUME_POPOVER_PADDING_H: i32 = 10;
pub const VOLUME_POPOVER_PADDING_V: i32 = 6;
pub const TIME_LABEL_MIN_WIDTH: i32 = 44;

// --- GTK Semantic Symbolic Icons ---
pub const ICON_SIDEBAR_TOGGLE: &str = "sidebar-show-symbolic";
pub const ICON_SIDEBAR_TOGGLE_ALT: &str = "sidebar-show-symbolic";

pub const ICON_HOME: &str = "go-home-symbolic";
pub const ICON_SEARCH: &str = "system-search-symbolic";
pub const ICON_NEW: &str = "view-grid-symbolic";
pub const ICON_RADIO: &str = "network-wireless-signal-excellent-symbolic";

pub const ICON_RECENTLY_ADDED: &str = "document-open-recent-symbolic";
pub const ICON_ARTISTS: &str = "audio-input-microphone-symbolic";
pub const ICON_ALBUMS: &str = "media-optical-cd-audio-symbolic";
pub const ICON_GENRES: &str = "tag-symbolic";
pub const ICON_SONGS: &str = "audio-x-generic-symbolic";
pub const ICON_PLAYLISTS: &str = "view-grid-symbolic";
pub const ICON_PLAYLIST_ITEM: &str = "playlist-symbolic";
pub const ICON_REPLAY: &str = "starred-symbolic";
pub const ICON_MADE_FOR_YOU: &str = "avatar-default-symbolic";
pub const ICON_SETTINGS: &str = "emblem-system-symbolic";
pub const ICON_USER: &str = "avatar-default-symbolic";

pub const ICON_PLAY: &str = "media-playback-start-symbolic";
pub const ICON_PAUSE: &str = "media-playback-pause-symbolic";
pub const ICON_PREVIOUS: &str = "media-skip-backward-symbolic";
pub const ICON_NEXT: &str = "media-skip-forward-symbolic";
pub const ICON_SHUFFLE: &str = "media-playlist-shuffle-symbolic";
pub const ICON_REPEAT: &str = "media-playlist-repeat-symbolic";

pub const ICON_QUEUE: &str = "view-list-symbolic";
pub const ICON_LYRICS: &str = "lyrics-quote-symbolic";
pub const ICON_FAVORITE: &str = "starred-symbolic";
pub const ICON_FAVORITE_OUTLINE: &str = "star-outline-symbolic";
pub const ICON_MORE: &str = "view-more-symbolic";
pub const ICON_ADD: &str = "list-add-symbolic";
pub const ICON_REMOVE: &str = "list-remove-symbolic";
pub const ICON_CLOSE: &str = "window-close-symbolic";

pub const ICON_VOLUME_HIGH: &str = "audio-volume-high-symbolic";
pub const ICON_VOLUME_MED: &str = "audio-volume-medium-symbolic";
pub const ICON_VOLUME_LOW: &str = "audio-volume-low-symbolic";
pub const ICON_VOLUME_MUTED: &str = "audio-volume-muted-symbolic";

pub const ICON_BACK: &str = "go-previous-symbolic";
pub const ICON_FORWARD: &str = "go-next-symbolic";
