//! Browser discovery and candidate classification for `malus-web-runtime`.

use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::error::WebError;

/// The rendering and script engine powering the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserEngine {
    Wpe,
    Chromium,
}

impl std::fmt::Display for BrowserEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wpe => write!(f, "WPE WebKit"),
            Self::Chromium => write!(f, "Chromium"),
        }
    }
}

/// User or environment preference for web runtime engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EnginePreference {
    #[default]
    Auto,
    Wpe,
    Chromium,
}

impl EnginePreference {
    pub fn from_env() -> Result<Self, WebError> {
        match env::var("MALUS_WEB_ENGINE").as_deref() {
            Ok("auto") | Ok("") | Err(env::VarError::NotPresent) => Ok(Self::Auto),
            Ok("wpe") => Ok(Self::Wpe),
            Ok("chromium") => Ok(Self::Chromium),
            Ok(other) => Err(WebError::Configuration(format!(
                "Invalid MALUS_WEB_ENGINE '{other}'. Expected 'auto', 'wpe', or 'chromium'"
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
    GoogleChrome,
    Chromium,
    Brave,
    MicrosoftEdge,
    Thorium,
    Vivaldi,
    Opera,
    UngoogledChromium,
    Custom(String),
}

/// How the browser binary is packaged and executed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LaunchMechanism {
    /// Native host executable invoked directly.
    Native,
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

/// A discovered browser candidate on the host system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserCandidate {
    pub display_name: String,
    pub engine: BrowserEngine,
    pub product: BrowserProduct,
    pub mechanism: LaunchMechanism,
    pub path: PathBuf,
    pub version: Option<String>,
    pub exec_cmd: Vec<String>,
}

impl BrowserCandidate {
    pub fn new(
        display_name: impl Into<String>,
        engine: BrowserEngine,
        product: BrowserProduct,
        mechanism: LaunchMechanism,
        path: PathBuf,
        version: Option<String>,
        exec_cmd: Vec<String>,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            engine,
            product,
            mechanism,
            path,
            version,
            exec_cmd,
        }
    }
}

/// Known native executable names to search in PATH, paired with their metadata.
struct KnownNative {
    bin_name: &'static str,
    label: &'static str,
    product: BrowserProduct,
    engine: BrowserEngine,
}

const KNOWN_NATIVE_SEARCH_MATRIX: &[KnownNative] = &[
    // Google Chrome family (Stable, Beta, Dev, Unstable, Canary)
    KnownNative {
        bin_name: "google-chrome-stable",
        label: "Google Chrome Stable",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "google-chrome",
        label: "Google Chrome",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "google-chrome-beta",
        label: "Google Chrome Beta",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "google-chrome-dev",
        label: "Google Chrome Dev",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "google-chrome-unstable",
        label: "Google Chrome Unstable",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "google-chrome-canary",
        label: "Google Chrome Canary",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "chrome",
        label: "Chrome",
        product: BrowserProduct::GoogleChrome,
        engine: BrowserEngine::Chromium,
    },
    // Brave family (Stable, Beta, Nightly, Dev)
    KnownNative {
        bin_name: "brave-browser-stable",
        label: "Brave Browser Stable",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave-browser",
        label: "Brave Browser",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave",
        label: "Brave",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave-browser-beta",
        label: "Brave Browser Beta",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave-beta",
        label: "Brave Beta",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave-browser-nightly",
        label: "Brave Browser Nightly",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave-nightly",
        label: "Brave Nightly",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "brave-browser-dev",
        label: "Brave Browser Dev",
        product: BrowserProduct::Brave,
        engine: BrowserEngine::Chromium,
    },
    // Microsoft Edge family (Stable, Beta, Dev, Canary)
    KnownNative {
        bin_name: "microsoft-edge-stable",
        label: "Microsoft Edge Stable",
        product: BrowserProduct::MicrosoftEdge,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "microsoft-edge",
        label: "Microsoft Edge",
        product: BrowserProduct::MicrosoftEdge,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "microsoft-edge-beta",
        label: "Microsoft Edge Beta",
        product: BrowserProduct::MicrosoftEdge,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "microsoft-edge-dev",
        label: "Microsoft Edge Dev",
        product: BrowserProduct::MicrosoftEdge,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "microsoft-edge-canary",
        label: "Microsoft Edge Canary",
        product: BrowserProduct::MicrosoftEdge,
        engine: BrowserEngine::Chromium,
    },
    // Thorium Browser & Niche Builds
    KnownNative {
        bin_name: "thorium-browser",
        label: "Thorium Browser",
        product: BrowserProduct::Thorium,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "thorium",
        label: "Thorium",
        product: BrowserProduct::Thorium,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "thorium-browser-avx2",
        label: "Thorium Browser (AVX2)",
        product: BrowserProduct::Thorium,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "thorium-browser-sse4",
        label: "Thorium Browser (SSE4)",
        product: BrowserProduct::Thorium,
        engine: BrowserEngine::Chromium,
    },
    // Vivaldi family
    KnownNative {
        bin_name: "vivaldi-stable",
        label: "Vivaldi Stable",
        product: BrowserProduct::Vivaldi,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "vivaldi",
        label: "Vivaldi",
        product: BrowserProduct::Vivaldi,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "vivaldi-snapshot",
        label: "Vivaldi Snapshot",
        product: BrowserProduct::Vivaldi,
        engine: BrowserEngine::Chromium,
    },
    // Opera family
    KnownNative {
        bin_name: "opera",
        label: "Opera",
        product: BrowserProduct::Opera,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "opera-beta",
        label: "Opera Beta",
        product: BrowserProduct::Opera,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "opera-developer",
        label: "Opera Developer",
        product: BrowserProduct::Opera,
        engine: BrowserEngine::Chromium,
    },
    // Chromium / Ungoogled Chromium
    KnownNative {
        bin_name: "chromium",
        label: "Chromium",
        product: BrowserProduct::Chromium,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "chromium-browser",
        label: "Chromium Browser",
        product: BrowserProduct::Chromium,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "ungoogled-chromium",
        label: "Ungoogled Chromium",
        product: BrowserProduct::UngoogledChromium,
        engine: BrowserEngine::Chromium,
    },
    KnownNative {
        bin_name: "ungoogled-chromium-bin",
        label: "Ungoogled Chromium Bin",
        product: BrowserProduct::UngoogledChromium,
        engine: BrowserEngine::Chromium,
    },
];

/// Discover installed native browser candidates on the host system.
///
/// Priority:
/// 1. Explicitly configured binary path (if supplied)
/// 2. `MALUS_BROWSER` environment variable
/// 3. PATH search matrix covering Chrome, Brave, Edge, Thorium, Vivaldi, Opera, Chromium, and niche builds
pub fn discover_browsers(custom_path: Option<&Path>) -> Vec<BrowserCandidate> {
    let mut candidates = Vec::new();
    let mut seen_canonical_paths = HashSet::new();

    // 1. Explicit path parameter
    if let Some(path) = custom_path
        && path.exists()
    {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let version = query_browser_version(path);
        let product = infer_product_from_name_or_version(
            &path.file_name().unwrap_or_default().to_string_lossy(),
            version.as_deref(),
        );
        candidates.push(BrowserCandidate::new(
            format!("Custom ({})", path.display()),
            BrowserEngine::Chromium,
            product,
            LaunchMechanism::Native,
            path.to_path_buf(),
            version,
            vec![path.to_string_lossy().to_string()],
        ));
        seen_canonical_paths.insert(canonical);
    }

    // 2. Environment variable: MALUS_BROWSER
    if let Ok(val) = env::var("MALUS_BROWSER") {
        let p = PathBuf::from(&val);
        let resolved = if p.exists() {
            Some(p)
        } else {
            which_in_path(&val)
        };

        if let Some(path) = resolved {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if seen_canonical_paths.insert(canonical) {
                let version = query_browser_version(&path);
                let product = infer_product_from_name_or_version(&val, version.as_deref());
                candidates.push(BrowserCandidate::new(
                    format!("Env $MALUS_BROWSER ({})", path.display()),
                    BrowserEngine::Chromium,
                    product,
                    LaunchMechanism::Native,
                    path.clone(),
                    version,
                    vec![path.to_string_lossy().to_string()],
                ));
            }
        }
    }

    // 3. Search matrix from PATH
    for known in KNOWN_NATIVE_SEARCH_MATRIX {
        if let Some(path) = which_in_path(known.bin_name) {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if seen_canonical_paths.insert(canonical) {
                let version = query_browser_version(&path);
                candidates.push(BrowserCandidate::new(
                    known.label,
                    known.engine,
                    known.product.clone(),
                    LaunchMechanism::Native,
                    path.clone(),
                    version,
                    vec![path.to_string_lossy().to_string()],
                ));
            }
        }
    }

    candidates
}

/// Select the preferred browser candidate.
///
/// If an explicit path is provided, it is returned if valid.
/// Otherwise, the highest-priority native candidate discovered is selected.
pub fn select_best_browser(custom_path: Option<&Path>) -> Result<BrowserCandidate, WebError> {
    if let Some(custom) = custom_path
        && !custom.exists()
    {
        return Err(WebError::ExplicitBrowserNotFound(custom.to_path_buf()));
    }

    let candidates = discover_browsers(custom_path);
    candidates
        .into_iter()
        .next()
        .ok_or(WebError::NoCompatibleBrowserFound)
}

/// Safely query `<binary> --version` with a 1.5s timeout to extract version information.
pub fn query_browser_version(binary_path: &Path) -> Option<String> {
    use std::sync::mpsc;
    use std::thread;

    let path = binary_path.to_path_buf();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let output = Command::new(&path).arg("--version").output();
        let _ = tx.send(output);
    });

    match rx.recv_timeout(Duration::from_millis(1500)) {
        Ok(Ok(output)) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

/// Find a binary in `$PATH`.
pub fn which_in_path(bin_name: &str) -> Option<PathBuf> {
    let p = Path::new(bin_name);
    if p.is_absolute() && p.exists() {
        return Some(p.to_path_buf());
    }

    if let Ok(path_val) = env::var("PATH") {
        for dir in env::split_paths(&path_val) {
            let candidate = dir.join(bin_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn infer_product_from_name_or_version(name: &str, version: Option<&str>) -> BrowserProduct {
    let combined = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        version.unwrap_or("").to_ascii_lowercase()
    );

    if combined.contains("brave") {
        BrowserProduct::Brave
    } else if combined.contains("edg") {
        BrowserProduct::MicrosoftEdge
    } else if combined.contains("thorium") {
        BrowserProduct::Thorium
    } else if combined.contains("vivaldi") {
        BrowserProduct::Vivaldi
    } else if combined.contains("opera") {
        BrowserProduct::Opera
    } else if combined.contains("ungoogled") {
        BrowserProduct::UngoogledChromium
    } else if combined.contains("google chrome") || combined.contains("chrome") {
        BrowserProduct::GoogleChrome
    } else if combined.contains("chromium") {
        BrowserProduct::Chromium
    } else {
        BrowserProduct::Custom(name.to_string())
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
/// 2. `MALUS_WPE_RUNTIME_DIR` environment variable (or legacy `MALUS_WPE_DIR`)
/// 3. Development runtime directory in workspace (`runtime/wpe`)
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
    } else if let Ok(legacy_env) = env::var("MALUS_WPE_DIR") {
        search_dirs.push(PathBuf::from(legacy_env));
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
