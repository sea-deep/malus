//! Browser auto-discovery matrix for Malus on Linux.
//!
//! Searches for native, Flatpak, and Snap installations of Chromium-based browsers,
//! prioritizing those with known, out-of-the-box Widevine L3 DRM support (Chrome, Brave, Edge).

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserKind {
    GoogleChrome,
    Brave,
    MicrosoftEdge,
    Thorium,
    Vivaldi,
    Opera,
    Chromium,
    Flatpak(String),
    Snap,
    Custom,
}

#[derive(Debug, Clone)]
pub struct BrowserCandidate {
    pub display_name: String,
    pub kind: BrowserKind,
    pub exec_cmd: Vec<String>,
    pub known_widevine: bool,
}

impl BrowserCandidate {
    pub fn new(
        display_name: impl Into<String>,
        kind: BrowserKind,
        exec_cmd: Vec<String>,
        known_widevine: bool,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            kind,
            exec_cmd,
            known_widevine,
        }
    }
}

/// Find all installed Chromium-based browser candidates on the system in priority order.
pub fn discover_browsers(custom_override: Option<&Path>) -> Vec<BrowserCandidate> {
    let mut candidates = Vec::new();

    // 1. Explicit CLI / Config override
    if let Some(custom) = custom_override
        && custom.exists()
    {
        candidates.push(BrowserCandidate::new(
            format!("Custom ({})", custom.display()),
            BrowserKind::Custom,
            vec![custom.to_string_lossy().to_string()],
            true, // User explicitly supplied this
        ));
    }

    // 2. Environment variables: MALUS_BROWSER, CHROME_BIN, BROWSER
    for env_var in &["MALUS_BROWSER", "CHROME_BIN", "BROWSER"] {
        if let Ok(val) = env::var(env_var) {
            let path = PathBuf::from(&val);
            if path.exists() || which_in_path(&val).is_some() {
                candidates.push(BrowserCandidate::new(
                    format!("Env ${} ({})", env_var, val),
                    BrowserKind::Custom,
                    vec![val],
                    true,
                ));
            }
        }
    }

    // 3. Google Chrome family (Highest out-of-the-box Widevine compatibility)
    let chrome_bins = [
        ("google-chrome-stable", "Google Chrome Stable"),
        ("google-chrome", "Google Chrome"),
        ("google-chrome-beta", "Google Chrome Beta"),
        ("google-chrome-unstable", "Google Chrome Unstable"),
        ("google-chrome-canary", "Google Chrome Canary"),
    ];
    for (bin, label) in chrome_bins {
        if let Some(path) = which_in_path(bin) {
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::GoogleChrome,
                vec![path.to_string_lossy().to_string()],
                true,
            ));
        }
    }

    // 4. Brave Browser family
    let brave_bins = [
        ("brave-browser-stable", "Brave Browser Stable"),
        ("brave-browser", "Brave Browser"),
        ("brave", "Brave"),
        ("brave-beta", "Brave Beta"),
        ("brave-nightly", "Brave Nightly"),
    ];
    for (bin, label) in brave_bins {
        if let Some(path) = which_in_path(bin) {
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::Brave,
                vec![path.to_string_lossy().to_string()],
                true,
            ));
        }
    }

    // 5. Microsoft Edge family
    let edge_bins = [
        ("microsoft-edge-stable", "Microsoft Edge Stable"),
        ("microsoft-edge", "Microsoft Edge"),
        ("microsoft-edge-beta", "Microsoft Edge Beta"),
        ("microsoft-edge-dev", "Microsoft Edge Dev"),
    ];
    for (bin, label) in edge_bins {
        if let Some(path) = which_in_path(bin) {
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::MicrosoftEdge,
                vec![path.to_string_lossy().to_string()],
                true,
            ));
        }
    }

    // 6. Thorium Browser
    for bin in &["thorium-browser", "thorium"] {
        if let Some(path) = which_in_path(bin) {
            candidates.push(BrowserCandidate::new(
                "Thorium Browser",
                BrowserKind::Thorium,
                vec![path.to_string_lossy().to_string()],
                true,
            ));
        }
    }

    // 7. Vivaldi
    for (bin, label) in [
        ("vivaldi-stable", "Vivaldi Stable"),
        ("vivaldi", "Vivaldi"),
        ("vivaldi-snapshot", "Vivaldi Snapshot"),
    ] {
        if let Some(path) = which_in_path(bin) {
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::Vivaldi,
                vec![path.to_string_lossy().to_string()],
                true,
            ));
        }
    }

    // 8. Opera
    for (bin, label) in [
        ("opera", "Opera"),
        ("opera-beta", "Opera Beta"),
        ("opera-developer", "Opera Developer"),
    ] {
        if let Some(path) = which_in_path(bin) {
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::Opera,
                vec![path.to_string_lossy().to_string()],
                true,
            ));
        }
    }

    // 9. Chromium / Ungoogled Chromium (May need chromium-widevine)
    for (bin, label) in [
        ("chromium", "Chromium"),
        ("chromium-browser", "Chromium Browser"),
        ("ungoogled-chromium", "Ungoogled Chromium"),
    ] {
        if let Some(path) = which_in_path(bin) {
            let widevine_present = has_chromium_widevine();
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::Chromium,
                vec![path.to_string_lossy().to_string()],
                widevine_present,
            ));
        }
    }

    // 10. Flatpak installations
    if which_in_path("flatpak").is_some() {
        let flatpaks = [
            ("com.google.Chrome", "Google Chrome (Flatpak)", true),
            ("com.brave.Browser", "Brave Browser (Flatpak)", true),
            ("com.microsoft.Edge", "Microsoft Edge (Flatpak)", true),
            ("org.chromium.Chromium", "Chromium (Flatpak)", false),
        ];
        for (app_id, label, widevine) in flatpaks {
            if is_flatpak_installed(app_id) {
                candidates.push(BrowserCandidate::new(
                    label,
                    BrowserKind::Flatpak(app_id.to_string()),
                    vec!["flatpak".to_string(), "run".to_string(), app_id.to_string()],
                    widevine,
                ));
            }
        }
    }

    // 11. Snap installations
    for (path_str, label, widevine) in [
        ("/snap/bin/google-chrome", "Google Chrome (Snap)", true),
        ("/snap/bin/brave", "Brave (Snap)", true),
        ("/snap/bin/chromium", "Chromium (Snap)", true),
    ] {
        let p = Path::new(path_str);
        if p.exists() {
            candidates.push(BrowserCandidate::new(
                label,
                BrowserKind::Snap,
                vec![path_str.to_string()],
                widevine,
            ));
        }
    }

    candidates
}

/// Select the best available browser candidate with known Widevine support,
/// falling back to the first available candidate if none are verified.
pub fn select_best_browser(custom_override: Option<&Path>) -> Option<BrowserCandidate> {
    let all = discover_browsers(custom_override);
    if let Some(candidate) = all.iter().find(|c| c.known_widevine) {
        return Some(candidate.clone());
    }
    all.into_iter().next()
}

fn which_in_path(bin_name: &str) -> Option<PathBuf> {
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

fn has_chromium_widevine() -> bool {
    let search_paths = [
        "/usr/lib/chromium/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        "/opt/google/chrome/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        "/usr/lib/chromium-browser/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
    ];
    search_paths.iter().any(|p| Path::new(p).exists())
}

fn is_flatpak_installed(app_id: &str) -> bool {
    Command::new("flatpak")
        .args(["info", app_id])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_browsers_finds_system_browser() {
        let browsers = discover_browsers(None);
        println!("Discovered {} browsers on this machine:", browsers.len());
        for b in &browsers {
            println!("  -> {} (widevine: {})", b.display_name, b.known_widevine);
        }
        assert!(
            !browsers.is_empty(),
            "Expected at least one Chromium-based browser to be discovered on this system"
        );
    }

    #[test]
    fn test_select_best_browser_prefers_widevine() {
        let best = select_best_browser(None);
        assert!(
            best.is_some(),
            "Expected best browser selection to find an installed browser"
        );
        let candidate = best.unwrap();
        println!(
            "Best selected browser: {} (cmd: {:?})",
            candidate.display_name, candidate.exec_cmd
        );
        assert!(
            candidate.known_widevine,
            "Expected best browser to have known Widevine support"
        );
    }
}
