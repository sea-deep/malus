//! Public runtime facade and backend supervisor.
//!
//! Exposes `WebRuntime` as an opaque facade concealing browser engine internals.

use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    discovery::{BrowserEngine, BrowserProduct, EnginePreference, discover_wpe},
    error::WebError,
    page::{PageHealth, WebPage},
    process::LaunchMode,
    wpe::WpeBackend,
};

/// Configuration options for launching a web runtime session.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Preferred engine (if None, determined by MALUS_WEB_ENGINE or defaults to Auto/Wpe).
    pub engine: Option<EnginePreference>,
    /// Window visibility (headed or headless). Defaults to Headed.
    pub launch_mode: LaunchMode,
    /// Initial URL to load on startup. Defaults to "about:blank".
    pub initial_url: String,
    /// Managed profile namespace under `$XDG_DATA_HOME/malus/profiles/<namespace>`.
    pub profile_namespace: Option<String>,
    /// Optional custom path for the profile directory (unmanaged).
    pub custom_profile_path: Option<PathBuf>,
    /// Additional command-line arguments to pass to the browser process.
    pub extra_args: Vec<String>,
    /// Whether to disable background timer and occlusion throttling (defaults to true).
    pub disable_background_throttling: bool,
    /// Optional explicit WPE directory (overrides MALUS_WPE_RUNTIME_DIR).
    pub wpe_dir: Option<PathBuf>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            engine: None,
            launch_mode: LaunchMode::Headed,
            initial_url: "about:blank".to_string(),
            profile_namespace: None,
            custom_profile_path: None,
            extra_args: Vec::new(),
            disable_background_throttling: true,
            wpe_dir: None,
        }
    }
}

/// Overall diagnostic health status of the running web runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub alive: bool,
    pub engine: BrowserEngine,
    pub product: BrowserProduct,
    pub pid: u32,
    pub port: Option<u16>,
    pub page: PageHealth,
    pub details: String,
}

/// Provider-facing opaque facade managing web execution.
///
/// Providers interact exclusively with `WebRuntime` and `WebPage`.
/// Concrete browser engines and communication mechanics are completely concealed.
pub struct WebRuntime {
    backend: WpeBackend,
    page: Arc<WebPage>,
}

impl WebRuntime {
    /// Launch a web runtime session according to configured options and engine selection rules.
    pub async fn launch(options: RuntimeOptions) -> Result<Self, WebError> {
        let preference = match options.engine {
            Some(p) => p,
            None => EnginePreference::from_env()?,
        };

        match preference {
            EnginePreference::Auto | EnginePreference::Wpe => {
                let wpe_candidate = discover_wpe(options.wpe_dir.as_deref()).ok_or_else(|| {
                    WebError::Initialization {
                        engine: BrowserEngine::Wpe,
                        message:
                            "WPE WebKit runtime candidate was not discovered on the host system"
                                .into(),
                    }
                })?;
                let backend = WpeBackend::launch(&wpe_candidate, options).await?;
                let page = Arc::new(WebPage::from_wpe(Arc::clone(backend.page())));
                info!("WPE WebKit runtime initialized successfully");
                Ok(Self { backend, page })
            }
        }
    }

    /// The browser engine powering this active runtime session.
    pub fn engine(&self) -> BrowserEngine {
        BrowserEngine::Wpe
    }

    /// Access the primary attached web page.
    pub fn page(&self) -> &WebPage {
        &self.page
    }

    /// Access a clone of the primary attached web page handle.
    pub fn page_handle(&self) -> Arc<WebPage> {
        Arc::clone(&self.page)
    }

    /// Query structured diagnostic health for the running runtime and page.
    pub async fn check_health(&self) -> Result<RuntimeHealth, WebError> {
        self.backend.check_health().await
    }

    /// Verify that DRM playback capabilities (such as Widevine CDM) are supported by the active engine.
    ///
    /// Asserts that Widevine CDM was successfully located on the host.
    pub fn ensure_drm_supported(&self) -> Result<(), WebError> {
        crate::widevine::discover_widevine()
            .map(|_| ())
            .map_err(|e| match e {
                crate::widevine::WidevineError::InvalidPath(msg) => WebError::Configuration(msg),
                _ => WebError::WidevineNotFound,
            })
    }

    /// Explicitly terminate the browser process and reap all child processes.
    pub async fn shutdown(self) -> Result<(), WebError> {
        self.backend.shutdown().await
    }
}
