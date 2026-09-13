//! # malus-web-runtime
//!
//! Provider-agnostic web runtime for Malus providers requiring browser execution.
//!
//! This crate provides:
//! - Discovered native WPE WebKit primary and Chromium fallback engines
//! - XDG-isolated and permission-hardened profile lifecycle
//! - Best-effort process group leadership, parent death signaling, and graceful shutdown escalation
//! - Capability-oriented facade (`WebRuntime`, `WebPage`, `WebEvent`) completely concealing
//!   underlying browser engines, CDP, and IPC mechanics

pub mod cdp;
pub mod discovery;
pub mod error;
pub mod page;
pub mod process;
pub mod profile;
pub mod runtime;
pub mod widevine;
pub mod wpe;

pub use discovery::{
    BrowserCandidate, BrowserEngine, BrowserProduct, EnginePreference, LaunchMechanism,
    WpeCandidate, discover_browsers, discover_wpe, select_best_browser,
};
pub use error::WebError;
pub use page::{PageHealth, WebEvent, WebPage};
pub use process::LaunchMode;
pub use profile::ProfileManager;
pub use runtime::{RuntimeHealth, RuntimeOptions, WebRuntime};
pub use widevine::{
    PersistedWidevineStatus, WidevineAcquisitionStrategy, WidevineError, WidevineInstallation,
    WidevineMetadata, WidevineResetResult, WidevineSource, check_persisted_widevine_status,
    discover_widevine, install_managed_widevine, persist_widevine_path,
    persist_widevine_path_to_file, read_managed_metadata, reset_widevine_config,
    reset_widevine_config_internal,
};
