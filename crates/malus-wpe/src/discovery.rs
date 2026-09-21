//! WPE WebKit discovery and runtime validation.

use std::{
    env,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::WebError;

/// The rendering and script engine powering the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserEngine {
    Wpe,
}

impl std::fmt::Display for BrowserEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wpe => write!(f, "WPE WebKit"),
        }
    }
}

/// User or environment preference for web runtime engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EnginePreference {
    #[default]
    Auto,
    Wpe,
}

impl EnginePreference {
    pub fn from_env() -> Result<Self, WebError> {
        match env::var("MALUS_WEB_ENGINE").as_deref() {
            Ok("auto") | Ok("") | Err(env::VarError::NotPresent) => Ok(Self::Auto),
            Ok("wpe") => Ok(Self::Wpe),
            Ok("chromium") => Err(WebError::Configuration(
                "Chromium engine is no longer supported; Malus requires WPE WebKit".into(),
            )),
            Ok(other) => Err(WebError::Configuration(format!(
                "Invalid MALUS_WEB_ENGINE '{other}'. Expected 'auto' or 'wpe'"
            ))),
            Err(e) => Err(WebError::Configuration(format!(
                "Failed to read MALUS_WEB_ENGINE: {e}"
            ))),
        }
    }
}

/// The specific browser distribution or branding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserProduct {
    WpeWebKit,
}

/// A discovered WPE WebKit runtime candidate on the host system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpeCandidate {
    pub display_name: String,
    pub binary_path: PathBuf,
    pub exec_path: Option<PathBuf>,
    pub library_paths: Vec<PathBuf>,
    pub ocdm_path: Option<PathBuf>,
    pub version: Option<String>,
}

impl WpeCandidate {
    pub fn new(
        display_name: impl Into<String>,
        binary_path: PathBuf,
        exec_path: Option<PathBuf>,
        library_paths: Vec<PathBuf>,
        ocdm_path: Option<PathBuf>,
        version: Option<String>,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            binary_path,
            exec_path,
            library_paths,
            ocdm_path,
            version,
        }
    }
}

/// Validate that a directory contains a complete, functional Malus WPE runtime layout.
pub fn validate_wpe_runtime(root: &Path) -> Result<WpeCandidate, WebError> {
    if !root.is_dir() {
        return Err(WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: format!("WPE runtime directory not found: {}", root.display()),
        });
    }

    let host_bin = root.join("bin/malus-wpe-host");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !host_bin.is_file()
            || !host_bin
                .metadata()
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        {
            return Err(WebError::Initialization {
                engine: BrowserEngine::Wpe,
                message: format!(
                    "Malus WPE host executable missing or not executable: {}",
                    host_bin.display()
                ),
            });
        }
    }
    #[cfg(not(unix))]
    {
        if !host_bin.is_file() {
            return Err(WebError::Initialization {
                engine: BrowserEngine::Wpe,
                message: format!("Malus WPE host binary not found: {}", host_bin.display()),
            });
        }
    }

    let web_process = root.join("bin/WPEWebProcess");
    if !web_process.is_file() {
        return Err(WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: format!(
                "WPEWebProcess binary missing from runtime: {}",
                web_process.display()
            ),
        });
    }

    let net_process = root.join("bin/WPENetworkProcess");
    if !net_process.is_file() {
        return Err(WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: format!(
                "WPENetworkProcess binary missing from runtime: {}",
                net_process.display()
            ),
        });
    }

    let lib_webkit = root.join("lib/libWPEWebKit-2.0.so.1");
    let lib_webkit_plain = root.join("lib/libWPEWebKit-2.0.so");
    if !lib_webkit.exists() && !lib_webkit_plain.exists() {
        return Err(WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: format!(
                "libWPEWebKit library missing from runtime: {}",
                lib_webkit.display()
            ),
        });
    }

    let ocdm_so = root.join("lib/libocdm.so");
    if !ocdm_so.is_file() {
        return Err(WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: format!(
                "OpenCDM shim library (libocdm.so) missing from runtime: {}",
                ocdm_so.display()
            ),
        });
    }

    // Diagnostic check for Widevine CDM
    if let Err(e) = crate::widevine::discover_widevine() {
        tracing::debug!(
            "Widevine CDM not currently available ({e}); protected media playback will require Widevine setup"
        );
    }

    let bin_dir = root.join("bin");
    let lib_dir = root.join("lib");

    Ok(WpeCandidate::new(
        format!("Malus WPE Host ({})", host_bin.display()),
        host_bin,
        Some(bin_dir),
        vec![lib_dir],
        Some(ocdm_so),
        Some("2.52.6".to_string()),
    ))
}

/// Discover WPE WebKit installation on the host system.
///
/// Priority:
/// 1. `custom_dir` if provided (e.g. from RuntimeOptions)
/// 2. `MALUS_WPE_RUNTIME_DIR` environment variable
/// 3. Current working directory runtime/wpe
/// 4. Workspace `runtime/wpe`
pub fn discover_wpe(custom_dir: Option<&Path>) -> Option<WpeCandidate> {
    if let Some(dir) = custom_dir {
        if dir.is_dir() {
            return validate_wpe_runtime(dir).ok();
        }
        return None;
    }

    let mut search_dirs = Vec::new();

    if let Ok(env_dir) = env::var("MALUS_WPE_RUNTIME_DIR") {
        search_dirs.push(PathBuf::from(env_dir));
    }

    // 1. Current working directory runtime/wpe
    if let Ok(cwd) = env::current_dir() {
        search_dirs.push(cwd.join("runtime/wpe"));
    }

    // 2. Relative to CARGO_MANIFEST_DIR during development
    let manifest_runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("runtime/wpe"));
    if let Some(mr) = manifest_runtime {
        search_dirs.push(mr);
    }

    for dir in search_dirs {
        if dir.is_dir() {
            match validate_wpe_runtime(&dir) {
                Ok(candidate) => return Some(candidate),
                Err(e) => {
                    tracing::debug!(
                        "WPE candidate at {} is not a valid runtime: {e}",
                        dir.display()
                    );
                }
            }
        }
    }

    None
}
