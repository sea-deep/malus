//! Canonical Widevine Content Decryption Module (`libwidevinecdm.so`) discovery and validation.
//!
//! Provides a single source of truth for locating, verifying, and managing Widevine installations
//! across developer environment overrides, user configuration, managed installs, and known browser/system directories.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

/// Official Google Linux Chrome stable package URL for amd64.
pub const GOOGLE_CHROME_DEB_URL: &str =
    "https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb";

/// Expected ELF header constants for architecture validation.
#[cfg(target_arch = "x86_64")]
pub const EXPECTED_ELF_CLASS: u8 = 2; // ELFCLASS64
#[cfg(target_arch = "x86_64")]
pub const EXPECTED_ELF_DATA: u8 = 1; // ELFDATA2LSB (Little Endian)
#[cfg(target_arch = "x86_64")]
pub const EXPECTED_ELF_MACHINE: u16 = 62; // EM_X86_64

#[cfg(target_arch = "aarch64")]
pub const EXPECTED_ELF_CLASS: u8 = 2; // ELFCLASS64
#[cfg(target_arch = "aarch64")]
pub const EXPECTED_ELF_DATA: u8 = 1; // ELFDATA2LSB (Little Endian)
#[cfg(target_arch = "aarch64")]
pub const EXPECTED_ELF_MACHINE: u16 = 183; // EM_AARCH64

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub const EXPECTED_ELF_CLASS: u8 = 2;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub const EXPECTED_ELF_DATA: u8 = 1;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub const EXPECTED_ELF_MACHINE: u16 = 0;

/// Structured error representing Widevine discovery, configuration, download, or extraction failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WidevineError {
    #[error(
        "Widevine CDM was not found. Apple Music playback requires Widevine. \
        Set MALUS_WIDEVINE_PATH to an existing libwidevinecdm.so, configure via 'malus setup-widevine', \
        or run 'malus setup-widevine --install'."
    )]
    NotFound,

    #[error(
        "Invalid MALUS_WIDEVINE_PATH: '{0}' does not exist or is not a valid Widevine CDM binary"
    )]
    InvalidPath(String),

    #[error("User configuration error: {0}")]
    Config(String),

    #[error("Download error: {0}")]
    Download(String),

    #[error("Extraction error: {0}")]
    Extraction(String),
}

/// Provenance of the discovered Widevine installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WidevineSource {
    EnvOverride,
    PersistedConfig,
    ManagedInstall,
    GoogleChrome,
    Chromium,
    Brave,
    Vivaldi,
    System,
}

impl std::fmt::Display for WidevineSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvOverride => write!(f, "Environment override ($MALUS_WIDEVINE_PATH)"),
            Self::PersistedConfig => write!(f, "Persisted user configuration"),
            Self::ManagedInstall => write!(f, "Managed install"),
            Self::GoogleChrome => write!(f, "Google Chrome"),
            Self::Chromium => write!(f, "Chromium"),
            Self::Brave => write!(f, "Brave"),
            Self::Vivaldi => write!(f, "Vivaldi"),
            Self::System => write!(f, "System directory"),
        }
    }
}

/// Extensible acquisition strategy boundary for Widevine installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidevineAcquisitionStrategy {
    ExistingInstalled,
    UserProvidedPath,
    ManagedInstall,
}

/// A validated, verified Widevine CDM installation on the host system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidevineInstallation {
    pub library_path: PathBuf,
    pub directory: PathBuf,
    pub source: WidevineSource,
}

/// Persisted Widevine user configuration shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WidevineUserConfig {
    pub library_path: Option<PathBuf>,
    pub source: Option<WidevineSource>,
}

/// Persisted configuration status for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistedWidevineStatus {
    NotConfigured,
    Valid(PathBuf),
    Invalid(PathBuf),
}

/// Metadata stored alongside a managed Widevine installation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidevineMetadata {
    pub source_url: String,
    pub version: String,
    pub architecture: String,
    pub installed_at: String,
}

/// Manifest structure declared inside Google's WidevineCdm package.
#[derive(Debug, Clone, Deserialize)]
struct WidevineManifest {
    pub version: Option<String>,
}

/// Result of a managed Widevine extraction from upstream deb package.
#[derive(Debug, Clone)]
pub struct ExtractedWidevine {
    pub version: String,
    pub library_path: PathBuf,
    pub manifest_path: PathBuf,
    pub license_path: Option<PathBuf>,
}

/// Result of a reset operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidevineResetResult {
    pub config_removed: bool,
    pub managed_files_removed: Option<PathBuf>,
    pub preserved_external_path: Option<PathBuf>,
}

/// Entry in the known candidate search matrix.
#[derive(Debug, Clone)]
pub struct KnownCandidate {
    pub path: PathBuf,
    pub source: WidevineSource,
}

pub const KNOWN_CANDIDATE_PATHS: &[(&str, WidevineSource)] = &[
    // Google Chrome family
    (
        "/opt/google/chrome/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::GoogleChrome,
    ),
    (
        "/opt/google/chrome/WidevineCdm/libwidevinecdm.so",
        WidevineSource::GoogleChrome,
    ),
    (
        "/opt/google/chrome-beta/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::GoogleChrome,
    ),
    (
        "/opt/google/chrome-unstable/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::GoogleChrome,
    ),
    (
        "/opt/google/chrome-canary/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::GoogleChrome,
    ),
    // Chromium family
    (
        "/usr/lib/chromium/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Chromium,
    ),
    (
        "/usr/lib/chromium/WidevineCdm/libwidevinecdm.so",
        WidevineSource::Chromium,
    ),
    (
        "/usr/lib/chromium-browser/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Chromium,
    ),
    (
        "/usr/lib/chromium-browser/WidevineCdm/libwidevinecdm.so",
        WidevineSource::Chromium,
    ),
    (
        "/usr/lib64/chromium/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Chromium,
    ),
    (
        "/usr/lib64/chromium-browser/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Chromium,
    ),
    // Brave family
    (
        "/opt/brave.com/brave/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Brave,
    ),
    (
        "/opt/brave.com/brave/WidevineCdm/libwidevinecdm.so",
        WidevineSource::Brave,
    ),
    (
        "/opt/brave.com/brave-beta/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Brave,
    ),
    (
        "/opt/brave.com/brave-nightly/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Brave,
    ),
    // Vivaldi family
    (
        "/opt/vivaldi/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Vivaldi,
    ),
    (
        "/opt/vivaldi/WidevineCdm/libwidevinecdm.so",
        WidevineSource::Vivaldi,
    ),
    (
        "/var/opt/vivaldi/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
        WidevineSource::Vivaldi,
    ),
    // System package locations
    ("/usr/lib/libwidevinecdm.so", WidevineSource::System),
    ("/usr/lib64/libwidevinecdm.so", WidevineSource::System),
    (
        "/usr/lib/widevine/libwidevinecdm.so",
        WidevineSource::System,
    ),
    (
        "/usr/lib64/widevine/libwidevinecdm.so",
        WidevineSource::System,
    ),
    (
        "/usr/lib/x86_64-linux-gnu/libwidevinecdm.so",
        WidevineSource::System,
    ),
];

/// Validate that a given path exists, is a regular file, is readable, and matches the host architecture ELF specification.
pub fn validate_widevine_library(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("Path is not a regular file: {}", path.display()));
    }

    let mut file = File::open(path)
        .map_err(|e| format!("Cannot open file for reading ({}): {e}", path.display()))?;

    let mut header = [0u8; 20];
    let bytes_read = file
        .read(&mut header)
        .map_err(|e| format!("Cannot read file ({}): {e}", path.display()))?;

    // Check ELF magic bytes: \x7f E L F
    if bytes_read < 4 || header[0..4] != [0x7f, b'E', b'L', b'F'] {
        return Err(format!(
            "File {} is not an ELF binary (invalid magic: {:?})",
            path.display(),
            &header[0..bytes_read.min(4)]
        ));
    }

    if bytes_read < 20 {
        return Err(format!(
            "Truncated ELF binary header in {}: read {bytes_read} bytes, expected at least 20",
            path.display()
        ));
    }

    // Check ELF class (32-bit vs 64-bit)
    let ei_class = header[4];
    if ei_class != EXPECTED_ELF_CLASS {
        return Err(format!(
            "Wrong ELF class in {}: got {ei_class}, expected {EXPECTED_ELF_CLASS} (64-bit)",
            path.display()
        ));
    }

    // Check endianness
    let ei_data = header[5];
    if ei_data != EXPECTED_ELF_DATA {
        return Err(format!(
            "Unsupported endianness in {}: got {ei_data}, expected {EXPECTED_ELF_DATA}",
            path.display()
        ));
    }

    // Check machine architecture (bytes 18..20)
    let e_machine = if ei_data == 1 {
        u16::from_le_bytes([header[18], header[19]])
    } else {
        u16::from_be_bytes([header[18], header[19]])
    };

    if e_machine != EXPECTED_ELF_MACHINE {
        return Err(format!(
            "Architecture mismatch in {}: ELF e_machine is {e_machine}, expected {EXPECTED_ELF_MACHINE}",
            path.display()
        ));
    }

    Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

/// Resolve candidate library file from either a direct `.so` path or a directory.
pub fn resolve_library_path_from_candidate(candidate: &Path) -> Option<PathBuf> {
    if candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    if candidate.is_dir() {
        let subpaths = [
            candidate.join("libwidevinecdm.so"),
            candidate.join("_platform_specific/linux_x64/libwidevinecdm.so"),
            candidate.join("WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so"),
            candidate.join("WidevineCdm/libwidevinecdm.so"),
        ];
        for sub in subpaths {
            if sub.is_file() {
                return Some(sub);
            }
        }
    }
    None
}

/// Determine the root directory to mount into sandbox for a discovered library.
pub fn resolve_sandbox_mount_dir(library_path: &Path) -> PathBuf {
    // If the path contains a "WidevineCdm" directory, mount that directory root
    let mut current = library_path.parent();
    while let Some(parent) = current {
        if parent.file_name().and_then(|n| n.to_str()) == Some("WidevineCdm") {
            return parent.to_path_buf();
        }
        current = parent.parent();
    }

    // Fall back to immediate parent directory
    library_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Return path to user configuration file `~/.config/malus/widevine.json`.
pub fn get_user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "malus", "malus").map(|p| p.config_dir().join("widevine.json"))
}

/// Return canonical root directory for Malus-managed Widevine installations: `$XDG_DATA_HOME/malus/widevine`.
pub fn get_managed_root() -> Option<PathBuf> {
    ProjectDirs::from("com", "malus", "malus").map(|p| p.data_dir().join("widevine"))
}

/// Sanitize a version string to prevent path traversal or special characters.
pub fn sanitize_version(version: &str) -> Result<String, WidevineError> {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return Err(WidevineError::Extraction("Version string is empty".into()));
    }
    if trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.contains('\0')
    {
        return Err(WidevineError::Extraction(format!(
            "Invalid characters in version string: '{version}'"
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(WidevineError::Extraction(format!(
            "Version contains non-standard characters: '{version}'"
        )));
    }
    Ok(trimmed.to_string())
}

/// Return directory for a specific managed Widevine version: `<managed_root>/<version>`.
pub fn get_managed_version_dir(version: &str) -> Result<PathBuf, WidevineError> {
    let sanitized = sanitize_version(version)?;
    let root = get_managed_root()
        .ok_or_else(|| WidevineError::Config("Could not determine XDG data directory".into()))?;
    Ok(root.join(sanitized))
}

/// Determine whether a given path is located inside the Malus-managed Widevine root directory.
pub fn is_managed_path(path: &Path) -> bool {
    let Some(managed_root) = get_managed_root() else {
        return false;
    };
    if let (Ok(canon_path), Ok(canon_root)) = (path.canonicalize(), managed_root.canonicalize()) {
        canon_path.starts_with(&canon_root)
    } else {
        path.starts_with(&managed_root)
    }
}

/// Read `metadata.json` from a managed Widevine directory if present.
pub fn read_managed_metadata(directory: &Path) -> Option<WidevineMetadata> {
    let meta_file = directory.join("metadata.json");
    if !meta_file.is_file() {
        return None;
    }
    let data = fs::read_to_string(&meta_file).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load persisted Widevine path if configured.
pub fn load_persisted_widevine_path() -> Option<PathBuf> {
    let cfg_path = get_user_config_path()?;
    if !cfg_path.is_file() {
        return None;
    }
    let data = fs::read_to_string(&cfg_path).ok()?;
    let cfg: WidevineUserConfig = serde_json::from_str(&data).ok()?;
    cfg.library_path
}

/// Check persisted Widevine configuration status for diagnostics.
pub fn check_persisted_widevine_status() -> PersistedWidevineStatus {
    match load_persisted_widevine_path() {
        None => PersistedWidevineStatus::NotConfigured,
        Some(path) => {
            if let Some(resolved) = resolve_library_path_from_candidate(&path)
                && validate_widevine_library(&resolved).is_ok()
            {
                PersistedWidevineStatus::Valid(resolved)
            } else {
                PersistedWidevineStatus::Invalid(path)
            }
        }
    }
}

/// Persist user-selected Widevine library path into `~/.config/malus/widevine.json`.
pub fn persist_widevine_path(path: &Path) -> Result<WidevineInstallation, WidevineError> {
    let source = if is_managed_path(path) {
        WidevineSource::ManagedInstall
    } else {
        WidevineSource::PersistedConfig
    };
    persist_widevine_path_with_source(path, source)
}

/// Persist Widevine library path with an explicit source variant.
pub fn persist_widevine_path_with_source(
    path: &Path,
    source: WidevineSource,
) -> Result<WidevineInstallation, WidevineError> {
    let cfg_path = get_user_config_path().ok_or_else(|| {
        WidevineError::Config("Could not resolve system configuration directory".into())
    })?;
    persist_widevine_path_to_file(path, source, &cfg_path)
}

/// Persist Widevine library path directly to a specified configuration file path.
pub fn persist_widevine_path_to_file(
    path: &Path,
    source: WidevineSource,
    cfg_path: &Path,
) -> Result<WidevineInstallation, WidevineError> {
    let resolved = resolve_library_path_from_candidate(path).ok_or_else(|| {
        WidevineError::InvalidPath(format!(
            "Path '{}' is not a valid libwidevinecdm.so binary or containing directory",
            path.display()
        ))
    })?;

    let valid_lib = validate_widevine_library(&resolved)
        .map_err(|e| WidevineError::InvalidPath(format!("Validation failed: {e}")))?;

    if let Some(parent) = cfg_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| WidevineError::Config(format!("Failed to create config dir: {e}")))?;
    }

    let cfg = WidevineUserConfig {
        library_path: Some(valid_lib.clone()),
        source: Some(source),
    };

    let data = serde_json::to_string_pretty(&cfg)
        .map_err(|e| WidevineError::Config(format!("Failed to serialize config: {e}")))?;

    fs::write(cfg_path, data)
        .map_err(|e| WidevineError::Config(format!("Failed to write config file: {e}")))?;

    let directory = resolve_sandbox_mount_dir(&valid_lib);
    Ok(WidevineInstallation {
        library_path: valid_lib,
        directory,
        source,
    })
}

/// Reset Widevine configuration. Removes persisted config and, if the configured path was Malus-managed,
/// deletes the managed files. External files are strictly preserved.
pub fn reset_widevine_config() -> Result<WidevineResetResult, WidevineError> {
    let cfg_path = get_user_config_path();
    let managed_root = get_managed_root();
    reset_widevine_config_internal(cfg_path.as_deref(), managed_root.as_deref())
}

/// Internal implementation of reset taking explicit configuration file and managed root paths.
pub fn reset_widevine_config_internal(
    cfg_path: Option<&Path>,
    managed_root: Option<&Path>,
) -> Result<WidevineResetResult, WidevineError> {
    let persisted = if let Some(p) = cfg_path {
        if p.is_file() {
            let data = fs::read_to_string(p).ok();
            data.and_then(|d| serde_json::from_str::<WidevineUserConfig>(&d).ok())
                .and_then(|c| c.library_path)
        } else {
            None
        }
    } else {
        None
    };

    let config_removed = if let Some(p) = cfg_path
        && p.is_file()
    {
        fs::remove_file(p)
            .map_err(|e| WidevineError::Config(format!("Failed removing config file: {e}")))?;
        true
    } else {
        false
    };

    let mut managed_files_removed = None;
    let mut preserved_external_path = None;

    if let Some(path) = persisted {
        let is_managed = if let Some(m_root) = managed_root {
            if let (Ok(canon_path), Ok(canon_root)) = (path.canonicalize(), m_root.canonicalize()) {
                canon_path.starts_with(&canon_root)
            } else {
                path.starts_with(m_root)
            }
        } else {
            false
        };

        if is_managed
            && let Some(parent) = path.parent()
            && let Some(m_root) = managed_root
            && let (Ok(canon_parent), Ok(canon_root)) =
                (parent.canonicalize(), m_root.canonicalize())
        {
            // Ensure parent is strictly a subdirectory of managed_root
            if canon_parent != canon_root && canon_parent.starts_with(&canon_root) {
                fs::remove_dir_all(&canon_parent).map_err(|e| {
                    WidevineError::Config(format!(
                        "Failed removing managed directory {}: {e}",
                        canon_parent.display()
                    ))
                })?;
                managed_files_removed = Some(canon_parent);
            }
        } else if !is_managed {
            preserved_external_path = Some(path);
        }
    }

    Ok(WidevineResetResult {
        config_removed,
        managed_files_removed,
        preserved_external_path,
    })
}

/// Discover Widevine CDM installation on the host system.
///
/// Discovery Order:
/// 1. Explicit `MALUS_WIDEVINE_PATH` environment override (fails fast if invalid)
/// 2. Persisted user configuration in `~/.config/malus/widevine.json`
/// 3. Known installed browser/system locations
/// 4. Returns `WidevineError::NotFound`
pub fn discover_widevine() -> Result<WidevineInstallation, WidevineError> {
    let env_override = std::env::var("MALUS_WIDEVINE_PATH").ok().map(PathBuf::from);
    let persisted = load_persisted_widevine_path();

    let candidate_list: Vec<KnownCandidate> = if std::env::var("MALUS_DISABLE_CANDIDATES").is_ok() {
        Vec::new()
    } else {
        KNOWN_CANDIDATE_PATHS
            .iter()
            .map(|(p, s)| KnownCandidate {
                path: PathBuf::from(p),
                source: *s,
            })
            .collect()
    };

    discover_widevine_internal(
        env_override.as_deref(),
        persisted.as_deref(),
        &candidate_list,
    )
}

/// Internal discovery implementation taking explicit sources for unit and integration testing.
pub fn discover_widevine_internal(
    env_path: Option<&Path>,
    persisted_path: Option<&Path>,
    candidate_list: &[KnownCandidate],
) -> Result<WidevineInstallation, WidevineError> {
    // 1. Explicit MALUS_WIDEVINE_PATH environment override
    if let Some(env_val) = env_path {
        debug!(
            "Evaluating MALUS_WIDEVINE_PATH override: {}",
            env_val.display()
        );
        let resolved = resolve_library_path_from_candidate(env_val).ok_or_else(|| {
            WidevineError::InvalidPath(format!(
                "MALUS_WIDEVINE_PATH is set to '{}', but no libwidevinecdm.so was found",
                env_val.display()
            ))
        })?;

        let valid_path = validate_widevine_library(&resolved).map_err(|e| {
            WidevineError::InvalidPath(format!(
                "MALUS_WIDEVINE_PATH is set to '{}', but validation failed: {e}",
                env_val.display()
            ))
        })?;

        let directory = resolve_sandbox_mount_dir(&valid_path);
        return Ok(WidevineInstallation {
            library_path: valid_path,
            directory,
            source: WidevineSource::EnvOverride,
        });
    }

    // 2. Persisted user configuration
    if let Some(persisted_val) = persisted_path {
        if let Some(resolved) = resolve_library_path_from_candidate(persisted_val)
            && let Ok(valid_path) = validate_widevine_library(&resolved)
        {
            let directory = resolve_sandbox_mount_dir(&valid_path);
            let source = if is_managed_path(&valid_path) {
                WidevineSource::ManagedInstall
            } else {
                WidevineSource::PersistedConfig
            };
            return Ok(WidevineInstallation {
                library_path: valid_path,
                directory,
                source,
            });
        }
        warn!(
            "Persisted Widevine path '{}' is no longer valid; falling back to known locations",
            persisted_val.display()
        );
    }

    // 3. Known installed browser and system candidate locations
    for candidate in candidate_list {
        if let Some(resolved) = resolve_library_path_from_candidate(&candidate.path)
            && let Ok(valid_path) = validate_widevine_library(&resolved)
        {
            let directory = resolve_sandbox_mount_dir(&valid_path);
            return Ok(WidevineInstallation {
                library_path: valid_path,
                directory,
                source: candidate.source,
            });
        }
    }

    Err(WidevineError::NotFound)
}

/// Extract Widevine CDM files (`libwidevinecdm.so`, `manifest.json`, `LICENSE`) from a Debian package stream.
///
/// Streams the `ar` archive and decompresses `data.tar.*` in memory, extracting *only* Widevine files into `dest_dir`.
/// Non-Widevine package contents (such as the large Chrome browser binary) are never written to disk.
pub fn extract_widevine_from_deb<R: Read>(
    deb_reader: R,
    dest_dir: &Path,
) -> Result<ExtractedWidevine, WidevineError> {
    fs::create_dir_all(dest_dir)
        .map_err(|e| WidevineError::Extraction(format!("Failed to create destination dir: {e}")))?;

    let mut archive = ar::Archive::new(deb_reader);
    let mut found_data_tar = false;
    let mut extracted_lib: Option<PathBuf> = None;
    let mut extracted_manifest: Option<PathBuf> = None;
    let mut extracted_license: Option<PathBuf> = None;

    while let Some(entry_result) = archive.next_entry() {
        let mut entry = entry_result
            .map_err(|e| WidevineError::Extraction(format!("Failed reading ar entry: {e}")))?;

        let id_raw = String::from_utf8_lossy(entry.header().identifier()).to_string();
        let identifier = id_raw.trim_end_matches('\0').trim_end_matches('/');

        if identifier.starts_with("data.tar") {
            found_data_tar = true;

            if identifier.ends_with(".xz") {
                let xz_decoder = xz2::read::XzDecoder::new(&mut entry);
                let mut tar_archive = tar::Archive::new(xz_decoder);

                let entries = tar_archive.entries().map_err(|e| {
                    WidevineError::Extraction(format!("Failed to read tar entries: {e}"))
                })?;

                for tar_entry_result in entries {
                    let mut tar_entry = tar_entry_result.map_err(|e| {
                        WidevineError::Extraction(format!("Failed reading tar entry: {e}"))
                    })?;

                    let path = tar_entry.path().map_err(|e| {
                        WidevineError::Extraction(format!("Failed reading tar path: {e}"))
                    })?;

                    let path_str = path.to_string_lossy();

                    // Check for libwidevinecdm.so
                    if path_str.ends_with("libwidevinecdm.so")
                        && tar_entry.header().entry_type().is_file()
                    {
                        if extracted_lib.is_some() {
                            return Err(WidevineError::Extraction(
                                "Multiple ambiguous libwidevinecdm.so binaries found in package"
                                    .into(),
                            ));
                        }

                        let out_path = dest_dir.join("libwidevinecdm.so");
                        let mut out_file = File::create(&out_path).map_err(|e| {
                            WidevineError::Extraction(format!(
                                "Failed to create libwidevinecdm.so output file: {e}"
                            ))
                        })?;

                        std::io::copy(&mut tar_entry, &mut out_file).map_err(|e| {
                            WidevineError::Extraction(format!(
                                "Failed writing libwidevinecdm.so: {e}"
                            ))
                        })?;

                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            fs::set_permissions(&out_path, fs::Permissions::from_mode(0o755)).ok();
                        }

                        extracted_lib = Some(out_path);
                    } else if path_str.ends_with("manifest.json")
                        && path_str.contains("Widevine")
                        && tar_entry.header().entry_type().is_file()
                    {
                        let out_path = dest_dir.join("manifest.json");
                        let mut out_file = File::create(&out_path).map_err(|e| {
                            WidevineError::Extraction(format!(
                                "Failed to create manifest.json output file: {e}"
                            ))
                        })?;

                        std::io::copy(&mut tar_entry, &mut out_file).map_err(|e| {
                            WidevineError::Extraction(format!("Failed writing manifest.json: {e}"))
                        })?;

                        extracted_manifest = Some(out_path);
                    } else if path_str.ends_with("LICENSE")
                        && path_str.contains("Widevine")
                        && tar_entry.header().entry_type().is_file()
                    {
                        let out_path = dest_dir.join("LICENSE");
                        let mut out_file = File::create(&out_path).map_err(|e| {
                            WidevineError::Extraction(format!(
                                "Failed to create LICENSE output file: {e}"
                            ))
                        })?;

                        std::io::copy(&mut tar_entry, &mut out_file).map_err(|e| {
                            WidevineError::Extraction(format!("Failed writing LICENSE: {e}"))
                        })?;

                        extracted_license = Some(out_path);
                    }
                }
            } else {
                return Err(WidevineError::Extraction(format!(
                    "Unsupported compression format in package member: {identifier}"
                )));
            }

            break;
        }
    }

    if !found_data_tar {
        return Err(WidevineError::Extraction(
            "Debian package did not contain a data.tar archive".into(),
        ));
    }

    let library_path = extracted_lib.ok_or_else(|| {
        WidevineError::Extraction("libwidevinecdm.so was not found in package".into())
    })?;

    let manifest_path = extracted_manifest.ok_or_else(|| {
        WidevineError::Extraction("manifest.json was not found in package".into())
    })?;

    // Parse version from manifest
    let manifest_data = fs::read_to_string(&manifest_path).map_err(|e| {
        WidevineError::Extraction(format!("Failed to read extracted manifest.json: {e}"))
    })?;

    let manifest_json: WidevineManifest = serde_json::from_str(&manifest_data).map_err(|e| {
        WidevineError::Extraction(format!("Failed to parse extracted manifest.json: {e}"))
    })?;

    let version = manifest_json.version.ok_or_else(|| {
        WidevineError::Extraction("manifest.json does not declare a version".into())
    })?;

    let sanitized_version = sanitize_version(&version)?;

    Ok(ExtractedWidevine {
        version: sanitized_version,
        library_path,
        manifest_path,
        license_path: extracted_license,
    })
}

/// Check if a redirect host is an approved Google-controlled endpoint.
pub fn is_allowed_google_host(host: &str) -> bool {
    host == "dl.google.com"
        || host == "google.com"
        || host.ends_with(".google.com")
        || host == "gvt1.com"
        || host.ends_with(".gvt1.com")
}

/// Generate a standard ISO 8601 UTC timestamp string without external crates.
fn current_iso8601() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();

    let sec = total_secs % 60;
    let min = (total_secs / 60) % 60;
    let hour = (total_secs / 3600) % 24;
    let mut days = total_secs / 86400;

    let mut year = 1970;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &dim in &month_days {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    let day = days + 1;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Acquire and install Widevine CDM from Google's official Linux package.
///
/// Flow:
/// 1. If not `force`, check if a valid managed installation already exists.
/// 2. Stream download package from official HTTPS source into a temporary file.
/// 3. Follow redirects only to approved Google hosts.
/// 4. Extract only `libwidevinecdm.so`, `manifest.json`, and `LICENSE` into a temporary staging dir.
/// 5. Validate ELF binary and host architecture.
/// 6. Write `metadata.json`.
/// 7. Atomically move staging files to `<managed_root>/<version>/`.
/// 8. Persist the final installed library path.
pub async fn install_managed_widevine(
    source_url: Option<&str>,
    force: bool,
) -> Result<WidevineInstallation, WidevineError> {
    let url = source_url.unwrap_or(GOOGLE_CHROME_DEB_URL);

    // 1. If not forcing, check if a valid managed installation already exists
    if !force
        && let Some(managed_root) = get_managed_root()
        && managed_root.is_dir()
        && let Ok(entries) = fs::read_dir(&managed_root)
    {
        for entry in entries.flatten() {
            let candidate_lib = entry.path().join("libwidevinecdm.so");
            if candidate_lib.is_file() && validate_widevine_library(&candidate_lib).is_ok() {
                debug!(
                    "Existing valid managed Widevine installation found at {}",
                    candidate_lib.display()
                );
                return persist_widevine_path_with_source(
                    &candidate_lib,
                    WidevineSource::ManagedInstall,
                );
            }
        }
    }

    // 2. Setup HTTP client with narrow Google redirect policy
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("Too many redirects");
            }
            let is_https = attempt.url().scheme() == "https";
            if !is_https {
                return attempt.error("Non-HTTPS redirect rejected");
            }
            let host_opt = attempt.url().host_str().map(|s| s.to_string());
            let Some(host) = host_opt else {
                return attempt.error("Missing host in redirect URL");
            };
            if !is_allowed_google_host(&host) {
                return attempt.error(format!("Redirect to unapproved host rejected: {host}"));
            }
            attempt.follow()
        }))
        .build()
        .map_err(|e| WidevineError::Download(format!("Failed to build HTTP client: {e}")))?;

    // 3. Download package into a temporary file
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| WidevineError::Download(format!("Failed to download from {url}: {e}")))?;

    if !response.status().is_success() {
        return Err(WidevineError::Download(format!(
            "Server returned HTTP error status: {}",
            response.status()
        )));
    }

    let mut temp_pkg = tempfile::NamedTempFile::new().map_err(|e| {
        WidevineError::Download(format!("Failed to create temporary download file: {e}"))
    })?;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| WidevineError::Download(format!("Download stream error: {e}")))?
    {
        temp_pkg
            .write_all(&chunk)
            .map_err(|e| WidevineError::Download(format!("Failed writing download chunk: {e}")))?;
    }

    temp_pkg
        .flush()
        .map_err(|e| WidevineError::Download(format!("Failed flushing download file: {e}")))?;

    // 4. Extract into temporary staging directory
    let stage_dir = tempfile::Builder::new()
        .prefix("malus-widevine-stage-")
        .tempdir()
        .map_err(|e| {
            WidevineError::Extraction(format!("Failed to create staging directory: {e}"))
        })?;

    let pkg_file = File::open(temp_pkg.path()).map_err(|e| {
        WidevineError::Extraction(format!("Failed to open downloaded package file: {e}"))
    })?;

    let extracted = extract_widevine_from_deb(pkg_file, stage_dir.path())?;

    // 5. Validate library and architecture
    validate_widevine_library(&extracted.library_path)
        .map_err(|e| WidevineError::Extraction(format!("Extracted CDM validation failed: {e}")))?;

    // 6. Write metadata.json
    let metadata = WidevineMetadata {
        source_url: url.to_string(),
        version: extracted.version.clone(),
        architecture: std::env::consts::ARCH.to_string(),
        installed_at: current_iso8601(),
    };

    let meta_json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| WidevineError::Extraction(format!("Failed to serialize metadata: {e}")))?;

    fs::write(stage_dir.path().join("metadata.json"), meta_json)
        .map_err(|e| WidevineError::Extraction(format!("Failed to write metadata.json: {e}")))?;

    // 7. Atomically move/install to destination directory: `<managed_root>/<version>/`
    let target_dir = get_managed_version_dir(&extracted.version)?;
    if let Some(parent) = target_dir.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            WidevineError::Config(format!("Failed to create managed root directory: {e}"))
        })?;
    }

    if target_dir.exists() && force {
        fs::remove_dir_all(&target_dir).map_err(|e| {
            WidevineError::Config(format!("Failed to clean existing managed directory: {e}"))
        })?;
    }

    fs::create_dir_all(&target_dir).map_err(|e| {
        WidevineError::Config(format!("Failed to create managed version directory: {e}"))
    })?;

    // Copy extracted files to target directory
    let files_to_install = [
        "libwidevinecdm.so",
        "manifest.json",
        "metadata.json",
        "LICENSE",
    ];

    for fname in files_to_install {
        let src = stage_dir.path().join(fname);
        if src.is_file() {
            let dst = target_dir.join(fname);
            fs::copy(&src, &dst).map_err(|e| {
                WidevineError::Config(format!("Failed to copy {fname} to {}: {e}", dst.display()))
            })?;
        }
    }

    let final_lib = target_dir.join("libwidevinecdm.so");

    // 8. Final validation before persistence
    validate_widevine_library(&final_lib).map_err(|e| {
        WidevineError::Config(format!("Final installed library validation failed: {e}"))
    })?;

    // 9. Persist selection
    persist_widevine_path_with_source(&final_lib, WidevineSource::ManagedInstall)
}
