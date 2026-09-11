//! Secure profile directory supervisor and isolation enforcer.
//!
//! Enforces:
//! - Validation of namespace identifiers against directory traversal attacks
//! - Strict POSIX `0700` (`rwx------`) permissions
//! - Rejection of symlinked profile roots
//! - Verified confinement to XDG profile directory before any deletion
//! - Conservative Chromium `SingletonLock` stale lock recovery

use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
};

use directories::ProjectDirs;

use crate::error::WebError;

/// Manages an isolated browser profile directory.
#[derive(Debug, Clone)]
pub struct ProfileManager {
    profile_dir: PathBuf,
    is_managed: bool,
    managed_root: Option<PathBuf>,
}

impl ProfileManager {
    /// Initialize a managed profile directory for a specific provider/namespace.
    ///
    /// Stores the profile under `$XDG_DATA_HOME/malus/profiles/<namespace>`
    /// (fallback: `~/.local/share/malus/profiles/<namespace>`).
    pub fn for_namespace(namespace: &str) -> Result<Self, WebError> {
        validate_namespace(namespace)?;

        let proj_dirs = ProjectDirs::from("com", "malus", "malus").ok_or_else(|| {
            WebError::Profile("Could not resolve system XDG data directory".to_string())
        })?;

        let managed_root = proj_dirs.data_local_dir().join("profiles");
        let profile_dir = managed_root.join(namespace);

        Ok(Self {
            profile_dir,
            is_managed: true,
            managed_root: Some(managed_root),
        })
    }

    /// Initialize a profile manager for an explicitly provided custom directory.
    ///
    /// Custom profiles are NOT eligible for automatic recursive wiping.
    pub fn with_custom_path(path: impl Into<PathBuf>) -> Self {
        Self {
            profile_dir: path.into(),
            is_managed: false,
            managed_root: None,
        }
    }

    /// Path to the profile directory.
    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    /// Path to the `DevToolsActivePort` file inside this profile.
    pub fn devtools_port_file(&self) -> PathBuf {
        self.profile_dir.join("DevToolsActivePort")
    }

    /// Whether this profile directory is managed inside Malus XDG storage.
    pub fn is_managed(&self) -> bool {
        self.is_managed
    }

    /// Prepare and validate the profile directory:
    /// - Checks that the directory is not a symlink
    /// - Creates directory recursively with `0700` mode if it doesn't exist
    /// - Sets permissions to `0700`
    /// - Performs conservative stale lock cleanup
    pub fn prepare_profile_dir(&self) -> Result<(), WebError> {
        // 1. Reject symlinks
        if fs::symlink_metadata(&self.profile_dir).is_ok()
            && self
                .profile_dir
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
        {
            return Err(WebError::Profile(format!(
                "Security violation: Profile directory {} is a symlink, which is prohibited.",
                self.profile_dir.display()
            )));
        }

        // 2. Create if missing with strict 0700
        if !self.profile_dir.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.profile_dir)
                .map_err(|e| {
                    WebError::Profile(format!(
                        "Failed to create profile dir {}: {}",
                        self.profile_dir.display(),
                        e
                    ))
                })?;
        }

        // 3. Enforce 0700 mode on existing directory
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(&self.profile_dir, perms).map_err(|e| {
            WebError::Profile(format!(
                "Failed to enforce 0700 permissions on {}: {}",
                self.profile_dir.display(),
                e
            ))
        })?;

        // 4. Stale lock recovery
        self.cleanup_stale_locks()?;

        // 5. Clean crash recovery state in Preferences to prevent "Restore pages?" dialogs
        self.sanitize_preferences();

        Ok(())
    }

    /// Reset any dirty exit flags in Chromium's Preferences file.
    pub fn sanitize_preferences(&self) {
        let pref_path = self.profile_dir.join("Default").join("Preferences");
        if let Ok(content) = fs::read_to_string(&pref_path) {
            let mut changed = false;
            let mut sanitized = content;
            if sanitized.contains(r#""exit_type":"Crashed""#) {
                sanitized =
                    sanitized.replace(r#""exit_type":"Crashed""#, r#""exit_type":"Normal""#);
                changed = true;
            }
            if sanitized.contains(r#""exited_cleanly":false"#) {
                sanitized =
                    sanitized.replace(r#""exited_cleanly":false"#, r#""exited_cleanly":true"#);
                changed = true;
            }
            if changed {
                let _ = fs::write(&pref_path, sanitized);
            }
        }
    }

    /// Conservatively check for and remove dead Chromium `SingletonLock` symlinks.
    pub fn cleanup_stale_locks(&self) -> Result<(), WebError> {
        let lock_path = self.profile_dir.join("SingletonLock");
        if !lock_path.exists() && fs::symlink_metadata(&lock_path).is_err() {
            return Ok(());
        }

        if let Ok(target) = fs::read_link(&lock_path) {
            let target_str = target.to_string_lossy();
            // Format is typically "<hostname>-<pid>"
            if let Some(pid_str) = target_str.split('-').next_back()
                && let Ok(pid) = pid_str.parse::<i32>()
            {
                if is_pid_alive_on_host(pid) {
                    return Err(WebError::Profile(format!(
                        "Another browser instance is already running with profile {} (PID: {})",
                        self.profile_dir.display(),
                        pid
                    )));
                } else {
                    // PID is dead: remove lock files
                    let _ = fs::remove_file(&lock_path);
                    let _ = fs::remove_file(self.profile_dir.join("SingletonCookie"));
                    let _ = fs::remove_file(self.profile_dir.join("SingletonSocket"));
                }
            }
        }

        Ok(())
    }

    /// Safely wipe the managed profile directory.
    ///
    /// Prohibits wiping custom unmanaged paths.
    /// Verifies the profile directory is strictly within the Malus-managed XDG profile root.
    pub fn wipe_managed(&self) -> Result<(), WebError> {
        if !self.is_managed {
            return Err(WebError::Profile(
                "Refusing to wipe unmanaged/custom profile path".to_string(),
            ));
        }

        let managed_root = self.managed_root.as_ref().ok_or_else(|| {
            WebError::Profile("Missing managed root for profile wipe".to_string())
        })?;

        if !self.profile_dir.exists() {
            return Ok(());
        }

        // Canonicalize both paths to ensure no symlink escape
        let canon_root = managed_root.canonicalize().map_err(|e| {
            WebError::Profile(format!(
                "Failed to canonicalize managed root {}: {}",
                managed_root.display(),
                e
            ))
        })?;
        let canon_dir = self.profile_dir.canonicalize().map_err(|e| {
            WebError::Profile(format!(
                "Failed to canonicalize profile dir {}: {}",
                self.profile_dir.display(),
                e
            ))
        })?;

        if !canon_dir.starts_with(&canon_root) || canon_dir == canon_root {
            return Err(WebError::Profile(format!(
                "Security violation: Profile path {} is not safely contained within managed root {}",
                canon_dir.display(),
                canon_root.display()
            )));
        }

        self.cleanup_stale_locks()?;
        fs::remove_dir_all(&self.profile_dir).map_err(|e| {
            WebError::Profile(format!(
                "Failed to remove profile dir {}: {}",
                self.profile_dir.display(),
                e
            ))
        })?;

        Ok(())
    }
}

/// Validate that a namespace contains only safe alphanumeric, dash, and underscore characters.
///
/// Prevents directory traversal (`..`, slashes, null bytes).
pub fn validate_namespace(namespace: &str) -> Result<(), WebError> {
    if namespace.is_empty() {
        return Err(WebError::Profile(
            "Profile namespace cannot be empty".to_string(),
        ));
    }

    if namespace.len() > 64 {
        return Err(WebError::Profile(
            "Profile namespace exceeds maximum length of 64 characters".to_string(),
        ));
    }

    for ch in namespace.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' {
            return Err(WebError::Profile(format!(
                "Invalid character '{}' in profile namespace '{}'. Only alphanumeric, '-' and '_' allowed.",
                ch, namespace
            )));
        }
    }

    Ok(())
}

/// Check if a process with `pid` is currently alive on the host.
fn is_pid_alive_on_host(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    unsafe {
        let res = libc::kill(pid, 0);
        if res == 0 {
            true
        } else {
            let err = std::io::Error::last_os_error();
            err.raw_os_error() != Some(libc::ESRCH)
        }
    }
}
