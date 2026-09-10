//! Profile directory supervisor and security enforcer for Malus.
//!
//! Enforces POSIX 0700 permissions, guards against symlink traversal,
//! and handles stale Chromium `SingletonLock` recovery.

use std::{
    fs,
    io::{self, ErrorKind},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
};

use directories::ProjectDirs;

pub struct ProfileManager {
    profile_dir: PathBuf,
}

impl ProfileManager {
    /// Initialize profile manager targeting `~/.local/share/malus/browser-profile`.
    pub fn new() -> io::Result<Self> {
        let proj_dirs = ProjectDirs::from("com", "malus", "malus").ok_or_else(|| {
            io::Error::new(ErrorKind::NotFound, "Could not resolve XDG data directory")
        })?;
        let profile_dir = proj_dirs.data_local_dir().join("browser-profile");
        Ok(Self { profile_dir })
    }

    pub fn with_custom_path(path: impl Into<PathBuf>) -> Self {
        Self {
            profile_dir: path.into(),
        }
    }

    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    /// Ensure directory exists with strict 0700 permissions and no symlink traversal.
    pub fn prepare_profile_dir(&self) -> io::Result<()> {
        if self.profile_dir.is_symlink() {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                format!(
                    "Security violation: Profile path {} is a symlink, which is prohibited.",
                    self.profile_dir.display()
                ),
            ));
        }

        if !self.profile_dir.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.profile_dir)?;
        }

        // Enforce 0700 (rwx------) ACL
        let permissions = fs::Permissions::from_mode(0o700);
        fs::set_permissions(&self.profile_dir, permissions)?;

        // Recover from any stale SingletonLock
        self.cleanup_stale_locks()?;

        Ok(())
    }

    /// Detect and remove stale Chromium `SingletonLock` and `SingletonCookie` left by prior crashes.
    pub fn cleanup_stale_locks(&self) -> io::Result<()> {
        let lock_path = self.profile_dir.join("SingletonLock");
        if lock_path.exists() || fs::symlink_metadata(&lock_path).is_ok() {
            if let Ok(target) = fs::read_link(&lock_path) {
                let target_str = target.to_string_lossy();
                // Format is usually hostname-pid
                if let Some(pid_str) = target_str.split('-').next_back()
                    && let Ok(pid) = pid_str.parse::<i32>()
                {
                    if !is_process_alive(pid) {
                        // Process is dead: remove stale lock files
                        let _ = fs::remove_file(&lock_path);
                        let _ = fs::remove_file(self.profile_dir.join("SingletonCookie"));
                        let _ = fs::remove_file(self.profile_dir.join("SingletonSocket"));
                    } else {
                        return Err(io::Error::new(
                            ErrorKind::AddrInUse,
                            format!(
                                "Another browser instance is already running with profile {} (PID: {})",
                                self.profile_dir.display(),
                                pid
                            ),
                        ));
                    }
                }
            } else {
                // If unreadable, attempt removal
                let _ = fs::remove_file(&lock_path);
            }
        }
        Ok(())
    }

    /// Path to DevToolsActivePort written by Chromium.
    pub fn devtools_port_file(&self) -> PathBuf {
        self.profile_dir.join("DevToolsActivePort")
    }

    /// Completely wipe the profile directory for secure logout.
    pub fn wipe_session(&self) -> io::Result<()> {
        if self.profile_dir.is_symlink() {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Refusing to erase a symlinked browser profile",
            ));
        }
        self.cleanup_stale_locks()?;
        if self.profile_dir.exists() {
            fs::remove_dir_all(&self.profile_dir)?;
        }
        Ok(())
    }
}

/// Check if a process with `pid` is alive using `kill(pid, 0)`.
fn is_process_alive(pid: i32) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn test_profile_permissions_are_strict_0700() {
        let temp_dir =
            std::env::temp_dir().join(format!("malus_test_profile_{}", std::process::id()));
        let pm = ProfileManager::with_custom_path(&temp_dir);
        pm.prepare_profile_dir()
            .expect("Failed to prepare profile dir");

        let meta = fs::metadata(pm.profile_dir()).expect("Failed to read metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "Profile directory must have strict 0700 permissions"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stale_singleton_lock_recovery() {
        let temp_dir = std::env::temp_dir().join(format!("malus_test_lock_{}", std::process::id()));
        let pm = ProfileManager::with_custom_path(&temp_dir);
        pm.prepare_profile_dir()
            .expect("Failed to prepare profile dir");

        let lock_path = pm.profile_dir().join("SingletonLock");
        // Create a symlink with a non-existent PID (999999)
        symlink("myhost-999999", &lock_path).expect("Failed to create dummy symlink");
        assert!(lock_path.exists() || fs::symlink_metadata(&lock_path).is_ok());

        // Calling cleanup_stale_locks should detect that PID 999999 is dead and remove the lock
        pm.cleanup_stale_locks()
            .expect("Failed to clean up stale locks");
        assert!(
            fs::symlink_metadata(&lock_path).is_err(),
            "Stale SingletonLock should be removed"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
