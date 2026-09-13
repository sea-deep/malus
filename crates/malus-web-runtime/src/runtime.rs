//! Public runtime facade and backend supervisor.
//!
//! Exposes `WebRuntime` as an opaque facade concealing browser engine internals.

use std::{path::PathBuf, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
    cdp::BrowserCdpClient,
    discovery::{
        BrowserCandidate, BrowserEngine, BrowserProduct, EnginePreference, discover_wpe,
        select_best_browser,
    },
    error::WebError,
    page::{ChromiumPageInner, PageHealth, WebPage},
    process::{BrowserProcess, LaunchMode, ProcessStatus},
    profile::ProfileManager,
    wpe::WpeBackend,
};

/// Configuration options for launching a web runtime session.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Preferred engine (if None, determined by MALUS_WEB_ENGINE or defaults to Auto).
    pub engine: Option<EnginePreference>,
    /// Window visibility (headed or headless). Defaults to Headed.
    pub launch_mode: LaunchMode,
    /// Initial URL to load on startup. Defaults to "about:blank".
    pub initial_url: String,
    /// Optional explicit path to the browser binary (overrides auto-discovery for Chromium).
    pub browser_override: Option<PathBuf>,
    /// Managed profile namespace under `$XDG_DATA_HOME/malus/profiles/<namespace>`.
    pub profile_namespace: Option<String>,
    /// Optional custom path for the profile directory (unmanaged).
    pub custom_profile_path: Option<PathBuf>,
    /// Additional command-line arguments to pass to the browser process.
    pub extra_args: Vec<String>,
    /// Whether to disable background timer and occlusion throttling (defaults to true).
    pub disable_background_throttling: bool,
    /// Optional explicit WPE directory (overrides MALUS_WPE_DIR).
    pub wpe_dir: Option<PathBuf>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            engine: None,
            launch_mode: LaunchMode::Headed,
            initial_url: "about:blank".to_string(),
            browser_override: None,
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

/// Internal backend implementation variant.
enum RuntimeBackend {
    Wpe(WpeBackend),
    Chromium(Box<ChromiumBackend>),
}

/// Internal Chromium backend supervisor.
struct ChromiumBackend {
    candidate: BrowserCandidate,
    process: BrowserProcess,
    client: BrowserCdpClient,
    page: Arc<WebPage>,
}

impl ChromiumBackend {
    async fn launch(options: RuntimeOptions) -> Result<Self, WebError> {
        let candidate = select_best_browser(options.browser_override.as_deref())?;

        let profile = if let Some(custom) = options.custom_profile_path {
            ProfileManager::with_custom_path(custom)
        } else if let Some(ns) = options.profile_namespace {
            ProfileManager::for_namespace(&ns)?
        } else {
            ProfileManager::for_namespace("default")?
        };

        let mut process = BrowserProcess::spawn(
            &candidate,
            &profile,
            options.launch_mode,
            &options.initial_url,
            &options.extra_args,
            options.disable_background_throttling,
        )
        .await?;

        let client = match BrowserCdpClient::connect(process.browser_ws_url()).await {
            Ok(c) => c,
            Err(e) => {
                let _ = process.shutdown().await;
                return Err(e);
            }
        };

        // Discover or create an active page target
        let targets = client.get_targets().await?;
        let page_target = targets.into_iter().find(|t| t.target_type == "page");

        let (target_id, needs_nav) = match page_target {
            Some(t) => {
                let needs = options.initial_url != "about:blank" && t.url != options.initial_url;
                (t.target_id, needs)
            }
            None => (client.create_target(&options.initial_url).await?, false),
        };

        let session_id = client.attach_to_target(&target_id).await?;
        let page_inner = ChromiumPageInner::attach(
            client.clone(),
            target_id,
            session_id,
            options.initial_url.clone(),
        )
        .await?;
        let page = Arc::new(WebPage::from_chromium(page_inner));

        // Only navigate if the discovered page target was not already opened with initial_url
        if needs_nav {
            let _ = page.navigate(&options.initial_url).await;
        }

        Ok(Self {
            candidate,
            process,
            client,
            page,
        })
    }

    async fn check_health(&self) -> Result<RuntimeHealth, WebError> {
        let status = self.process.check_status()?;
        let page_health = self.page.check_health().await?;

        let (alive, details) = match status {
            ProcessStatus::Running { pid, pgid } => {
                if self.client.is_connected() && page_health.connected {
                    (true, format!("Running (PID: {}, PGID: {})", pid, pgid))
                } else {
                    (false, "Process running but CDP disconnected".to_string())
                }
            }
            ProcessStatus::Exited { exit_code } => {
                (false, format!("Process exited (code: {:?})", exit_code))
            }
        };

        Ok(RuntimeHealth {
            alive,
            engine: self.candidate.engine,
            product: self.candidate.product.clone(),
            pid: self.process.pid(),
            port: Some(self.process.port()),
            page: page_health,
            details,
        })
    }

    async fn shutdown(mut self) -> Result<(), WebError> {
        if self.client.is_connected() {
            let _ = self
                .client
                .send_command(
                    None,
                    "Browser.close",
                    serde_json::json!({}),
                    Duration::from_millis(3000),
                )
                .await;
        }
        self.process.shutdown().await
    }
}

/// Provider-facing opaque facade managing web execution.
///
/// Providers interact exclusively with `WebRuntime` and `WebPage`.
/// Concrete browser engines (WPE WebKit or Chromium) and communication mechanics
/// are completely concealed.
pub struct WebRuntime {
    engine: BrowserEngine,
    backend: RuntimeBackend,
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
            EnginePreference::Auto => {
                info!("Auto engine selection: attempting primary WPE WebKit runtime...");
                if let Some(wpe_candidate) = discover_wpe(options.wpe_dir.as_deref()) {
                    match WpeBackend::launch(&wpe_candidate, options.clone()).await {
                        Ok(wpe_backend) => {
                            let page = Arc::new(WebPage::from_wpe(Arc::clone(wpe_backend.page())));
                            info!("WPE WebKit runtime successfully initialized as primary target");
                            return Ok(Self {
                                engine: BrowserEngine::Wpe,
                                backend: RuntimeBackend::Wpe(wpe_backend),
                                page,
                            });
                        }
                        Err(e) => {
                            warn!(
                                "WPE WebKit failed to initialize ({e}); falling back to Chromium..."
                            );
                        }
                    }
                } else {
                    warn!("WPE WebKit candidate not discovered; falling back to Chromium...");
                }

                let chromium_backend = ChromiumBackend::launch(options).await?;
                let page = Arc::clone(&chromium_backend.page);
                info!("Chromium fallback runtime initialized successfully");
                Ok(Self {
                    engine: BrowserEngine::Chromium,
                    backend: RuntimeBackend::Chromium(Box::new(chromium_backend)),
                    page,
                })
            }
            EnginePreference::Wpe => {
                let wpe_candidate = discover_wpe(options.wpe_dir.as_deref()).ok_or_else(|| {
                    WebError::Initialization {
                        engine: BrowserEngine::Wpe,
                        message: "Explicit MALUS_WEB_ENGINE=wpe requested, but no WPE candidate was discovered".into(),
                    }
                })?;
                let wpe_backend = WpeBackend::launch(&wpe_candidate, options).await?;
                let page = Arc::new(WebPage::from_wpe(Arc::clone(wpe_backend.page())));
                info!("Explicit WPE WebKit runtime initialized successfully");
                Ok(Self {
                    engine: BrowserEngine::Wpe,
                    backend: RuntimeBackend::Wpe(wpe_backend),
                    page,
                })
            }
            EnginePreference::Chromium => {
                let chromium_backend = ChromiumBackend::launch(options).await?;
                let page = Arc::clone(&chromium_backend.page);
                info!("Explicit Chromium runtime initialized successfully");
                Ok(Self {
                    engine: BrowserEngine::Chromium,
                    backend: RuntimeBackend::Chromium(Box::new(chromium_backend)),
                    page,
                })
            }
        }
    }

    /// The browser engine powering this active runtime session.
    pub fn engine(&self) -> BrowserEngine {
        self.engine
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
        match &self.backend {
            RuntimeBackend::Wpe(b) => b.check_health().await,
            RuntimeBackend::Chromium(b) => b.check_health().await,
        }
    }

    /// Verify that DRM playback capabilities (such as Widevine CDM) are supported by the active engine.
    ///
    /// For WPE WebKit, this asserts that Widevine CDM was successfully located on the host.
    /// For Chromium, DRM playback uses internal/packaged browser CDM capabilities.
    pub fn ensure_drm_supported(&self) -> Result<(), WebError> {
        match self.engine {
            BrowserEngine::Wpe => crate::widevine::discover_widevine()
                .map(|_| ())
                .map_err(|e| match e {
                    crate::widevine::WidevineError::InvalidPath(msg) => {
                        WebError::Configuration(msg)
                    }
                    _ => WebError::WidevineNotFound,
                }),
            BrowserEngine::Chromium => Ok(()),
        }
    }

    /// Explicitly terminate the browser process and reap all child processes.
    pub async fn shutdown(self) -> Result<(), WebError> {
        match self.backend {
            RuntimeBackend::Wpe(b) => b.shutdown().await,
            RuntimeBackend::Chromium(b) => b.shutdown().await,
        }
    }
}
