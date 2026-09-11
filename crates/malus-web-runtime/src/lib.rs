//! # malus-web-runtime
//!
//! Provider-agnostic web runtime for Malus providers requiring browser execution.
//!
//! This crate provides:
//! - Discovered native Chromium-family browser selection
//! - XDG-isolated and permission-hardened profile lifecycle
//! - Best-effort process group leadership, parent death signaling, and graceful shutdown escalation
//! - Direct browser-level WebSocket CDP management (`Target.*` flattening)
//! - Backend-neutral facade (`WebRuntime`, `WebPage`, `WebEvent`) designed to permit future
//!   alternative backends without API changes

pub mod cdp;
pub mod discovery;
pub mod error;
pub mod page;
pub mod process;
pub mod profile;
pub mod runtime;

pub use discovery::{
    BrowserCandidate, BrowserEngine, BrowserProduct, LaunchMechanism, discover_browsers,
    select_best_browser,
};
pub use error::WebError;
pub use page::{PageHealth, WebEvent, WebPage};
pub use process::LaunchMode;
pub use profile::ProfileManager;
pub use runtime::{RuntimeHealth, RuntimeOptions, WebRuntime};
