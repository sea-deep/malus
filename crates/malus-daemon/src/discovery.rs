//! Generic provider executable discovery for `malus-daemon`.
//!
//! Searches for provider executables matching the naming convention `malus-provider-<id>`
//! across:
//! 1. Explicitly configured provider path (highest priority)
//! 2. Directory containing the daemon executable (`current_exe().parent()`)
//! 3. `MALUS_PROVIDER_PATH` (colon-separated list of directories)
//! 4. System `$PATH`
//!
//! Invariants:
//! - Discovery is side-effect-free: discovering does NOT spawn any processes (`installed != running`)
//! - Deduplication by canonical path and by provider ID (earlier search source wins)
//! - Executables only; non-executable files or directories are ignored
//! - No XDG data directory executable scanning

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};
use tracing::{debug, info};

pub const PROVIDER_PREFIX: &str = "malus-provider-";

/// A discovered provider binary on the host system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredProvider {
    pub id: String,
    pub executable: PathBuf,
}

/// Discover available provider executables on the host.
///
/// Returns a map of `id -> DiscoveredProvider`, ordered by search priority.
pub fn discover_providers(explicit_override: Option<&Path>) -> HashMap<String, DiscoveredProvider> {
    let mut discovered: HashMap<String, DiscoveredProvider> = HashMap::new();
    let mut seen_canonical_paths: HashSet<PathBuf> = HashSet::new();

    // 1. Explicit provider binary override (highest priority)
    if let Some(explicit) = explicit_override
        && let Some(provider) = inspect_candidate_file(explicit)
    {
        let canonical = explicit
            .canonicalize()
            .unwrap_or_else(|_| explicit.to_path_buf());
        seen_canonical_paths.insert(canonical);
        info!(
            "Discovered provider '{}' from explicit path: {}",
            provider.id,
            provider.executable.display()
        );
        discovered.insert(provider.id.clone(), provider);
    }

    // 2. Directory containing the current daemon executable
    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        scan_directory(exe_dir, &mut discovered, &mut seen_canonical_paths);
    }

    // 3. MALUS_PROVIDER_PATH environment variable (colon-separated)
    if let Ok(paths_val) = env::var("MALUS_PROVIDER_PATH") {
        for dir in env::split_paths(&paths_val) {
            scan_directory(&dir, &mut discovered, &mut seen_canonical_paths);
        }
    }

    // 4. System $PATH
    if let Ok(path_val) = env::var("PATH") {
        for dir in env::split_paths(&path_val) {
            scan_directory(&dir, &mut discovered, &mut seen_canonical_paths);
        }
    }

    discovered
}

/// Extract provider ID from a filename matching `malus-provider-<id>`.
pub fn extract_provider_id(filename: &str) -> Option<String> {
    if let Some(id) = filename.strip_prefix(PROVIDER_PREFIX)
        && !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(id.to_string());
    }
    None
}

fn scan_directory(
    dir: &Path,
    discovered: &mut HashMap<String, DiscoveredProvider>,
    seen_canonical_paths: &mut HashSet<PathBuf>,
) {
    if !dir.is_dir() {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(provider) = inspect_candidate_file(&path) {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

            // Avoid duplicate executables (e.g. symlinks)
            if seen_canonical_paths.contains(&canonical) {
                continue;
            }

            // If an ID was already discovered by a higher-priority source, ignore subsequent duplicates
            if discovered.contains_key(&provider.id) {
                seen_canonical_paths.insert(canonical);
                continue;
            }

            debug!(
                "Discovered provider '{}' at {}",
                provider.id,
                provider.executable.display()
            );
            seen_canonical_paths.insert(canonical);
            discovered.insert(provider.id.clone(), provider);
        }
    }
}

fn inspect_candidate_file(path: &Path) -> Option<DiscoveredProvider> {
    if !path.is_file() {
        return None;
    }

    let file_name = path.file_name()?.to_str()?;
    let id = extract_provider_id(file_name)?;

    // Check executable permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = path.metadata()
            && meta.permissions().mode() & 0o111 == 0
        {
            return None;
        }
    }

    Some(DiscoveredProvider {
        id,
        executable: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_extract_provider_id() {
        assert_eq!(
            extract_provider_id("malus-provider-mock"),
            Some("mock".to_string())
        );
        assert_eq!(
            extract_provider_id("malus-provider-apple"),
            Some("apple".to_string())
        );
        assert_eq!(
            extract_provider_id("malus-provider-spotify"),
            Some("spotify".to_string())
        );
        assert_eq!(
            extract_provider_id("malus-provider-custom_1"),
            Some("custom_1".to_string())
        );

        // Invalid
        assert_eq!(extract_provider_id("malus-provider-"), None);
        assert_eq!(extract_provider_id("malus-daemon"), None);
        assert_eq!(extract_provider_id("provider-mock"), None);
        assert_eq!(extract_provider_id("malus-provider-foo/bar"), None);
    }

    #[test]
    fn test_scan_directory_discovers_executable() {
        let temp = tempfile::tempdir().unwrap();
        let bin_path = temp.path().join("malus-provider-testfoo");
        fs::write(&bin_path, "#!/bin/sh\nexit 0").unwrap();

        // Make executable
        let mut perms = fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin_path, perms).unwrap();

        let mut discovered = HashMap::new();
        let mut seen = HashSet::new();
        scan_directory(temp.path(), &mut discovered, &mut seen);

        assert!(discovered.contains_key("testfoo"));
        assert_eq!(discovered["testfoo"].id, "testfoo");
    }

    #[test]
    fn test_non_executable_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let bin_path = temp.path().join("malus-provider-testbar");
        fs::write(&bin_path, "not an executable").unwrap();

        // Mode 0644 (not executable)
        let mut perms = fs::metadata(&bin_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&bin_path, perms).unwrap();

        let mut discovered = HashMap::new();
        let mut seen = HashSet::new();
        scan_directory(temp.path(), &mut discovered, &mut seen);

        assert!(!discovered.contains_key("testbar"));
    }
}
