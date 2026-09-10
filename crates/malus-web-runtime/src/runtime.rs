//! Public runtime facade and backend supervisor.
//!
//! Exposes `WebRuntime` as an opaque facade concealing browser engine internals.
//! Designed so alternative backends (such as Gecko) can be introduced without modifying
//! the provider-facing API.

use std::{path::PathBuf, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    cdp::BrowserCdpClient,
    discovery::{BrowserCandidate, BrowserEngine, BrowserProduct, select_best_browser},
    error::WebError,
    page::{PageHealth, WebPage},
    process::{BrowserProcess, LaunchMode, ProcessStatus},
    profile::ProfileManager,
};

/// Configuration options for launching a web runtime session.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Window visibility (headed or headless). Defaults to Headed.
    pub launch_mode: LaunchMode,
    /// Initial URL to load on startup. Defaults to "about:blank".
    pub initial_url: String,
    /// Optional explicit path to the browser binary (overrides auto-discovery).
    pub browser_override: Option<PathBuf>,
    /// Managed profile namespace under `$XDG_DATA_HOME/malus/profiles/<namespace>`.
    pub profile_namespace: Option<String>,
    /// Optional custom path for the profile directory (unmanaged).
    pub custom_profile_path: Option<PathBuf>,
    /// Additional command-line arguments to pass to the browser process.
    pub extra_args: Vec<String>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            launch_mode: LaunchMode::Headed,
            initial_url: "about:blank".to_string(),
            browser_override: None,
            profile_namespace: None,
            custom_profile_path: None,
            extra_args: Vec::new(),
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
    pub port: u16,
    pub page: PageHealth,
    pub details: String,
}

/// Internal backend implementation variant.
enum RuntimeBackend {
    Chromium(ChromiumBackend),
    // Future backend variants can be added here without altering WebRuntime:
    // Gecko(GeckoBackend),
}

/// Internal Chromium backend supervisor.
struct ChromiumBackend {
    candidate: BrowserCandidate,
    process: BrowserProcess,
    client: BrowserCdpClient,
    page: Arc<WebPage>,
}

impl ChromiumBackend {
    async fn launch(
        candidate: &BrowserCandidate,
        options: RuntimeOptions,
    ) -> Result<Self, WebError> {
        let profile = if let Some(custom) = options.custom_profile_path {
            ProfileManager::with_custom_path(custom)
        } else if let Some(ns) = options.profile_namespace {
            ProfileManager::for_namespace(&ns)?
        } else {
            ProfileManager::for_namespace("default")?
        };

        let mut process = BrowserProcess::spawn(
            candidate,
            &profile,
            options.launch_mode,
            &options.initial_url,
            &options.extra_args,
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

        let target_id = match page_target {
            Some(t) => t.target_id,
            None => client.create_target(&options.initial_url).await?,
        };

        let session_id = client.attach_to_target(&target_id).await?;
        let page = Arc::new(WebPage::attach(client.clone(), target_id, session_id).await?);

        // If an initial URL other than blank was requested and target was already present, navigate
        if options.initial_url != "about:blank" {
            let _ = page.navigate(&options.initial_url).await;
        }

        Ok(Self {
            candidate: candidate.clone(),
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
            port: self.process.port(),
            page: page_health,
            details,
        })
    }

    async fn shutdown(mut self) -> Result<(), WebError> {
        // Issue graceful Browser.close command over CDP if connected so Chromium flushes cookies/storage.
        if self.client.is_connected() {
            let _ = self
                .client
                .send_command(
                    None,
                    "Browser.close",
                    serde_json::json!({}),
                    Duration::from_millis(1500),
                )
                .await;
        }
        self.process.shutdown().await
    }
}

/// Provider-facing opaque facade managing web execution.
///
/// Providers interact exclusively with `WebRuntime` and `WebPage`.
/// Concrete browser engines (Chromium, Gecko) and CDP internals are hidden.
pub struct WebRuntime {
    backend: RuntimeBackend,
}

impl WebRuntime {
    /// Launch a web runtime session using the configured options and discovered system browser.
    pub async fn launch(options: RuntimeOptions) -> Result<Self, WebError> {
        let candidate = select_best_browser(options.browser_override.as_deref())?;

        match candidate.engine {
            BrowserEngine::Chromium => {
                let backend = ChromiumBackend::launch(&candidate, options).await?;
                Ok(Self {
                    backend: RuntimeBackend::Chromium(backend),
                })
            }
            BrowserEngine::Gecko => Err(WebError::Launch(
                "Gecko backend is not yet implemented".to_string(),
            )),
        }
    }

    /// Access the primary attached web page.
    pub fn page(&self) -> &WebPage {
        match &self.backend {
            RuntimeBackend::Chromium(b) => &b.page,
        }
    }

    /// Access a clone of the primary attached web page handle.
    pub fn page_handle(&self) -> Arc<WebPage> {
        match &self.backend {
            RuntimeBackend::Chromium(b) => Arc::clone(&b.page),
        }
    }

    /// Information about the active browser candidate executing this runtime.
    pub fn candidate(&self) -> &BrowserCandidate {
        match &self.backend {
            RuntimeBackend::Chromium(b) => &b.candidate,
        }
    }

    /// Query structured diagnostic health for the running runtime and page.
    pub async fn check_health(&self) -> Result<RuntimeHealth, WebError> {
        match &self.backend {
            RuntimeBackend::Chromium(b) => b.check_health().await,
        }
    }

    /// Explicitly terminate the browser process and reap all child processes.
    pub async fn shutdown(self) -> Result<(), WebError> {
        match self.backend {
            RuntimeBackend::Chromium(b) => b.shutdown().await,
        }
    }
}
