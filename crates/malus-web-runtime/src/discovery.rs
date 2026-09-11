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
    Chromium,
}

/// The specific browser distribution or branding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserProduct {
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
