//! Rotating log file writer and dual stdout/file subscriber support for Malus daemon.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// Default maximum size for an active log file: 5 MB.
/// Combined with a single rotated backup (`.1`), total disk usage is capped at 10 MB.
pub const DEFAULT_MAX_LOG_SIZE: u64 = 5 * 1024 * 1024;

/// Generates a backup path for log rotation (e.g. `malusd.log.1`).
fn backup_path(path: &Path) -> PathBuf {
    let mut filename = path.file_name().unwrap_or_default().to_os_string();
    filename.push(".1");
    path.with_file_name(filename)
}

/// A file writer that automatically rotates to a single backup (`.1`) when it reaches `max_file_size`.
pub struct RotatingFile {
    path: PathBuf,
    max_file_size: u64,
    file: Option<File>,
    current_size: u64,
}

impl RotatingFile {
    /// Creates a new `RotatingFile` pointing to `path`.
    ///
    /// If the file already exists and exceeds `max_file_size`, it is immediately rotated.
    pub fn new(path: PathBuf, max_file_size: u64) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let current_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        let mut rotating = Self {
            path,
            max_file_size,
            file: None,
            current_size,
        };

        if current_size >= max_file_size {
            rotating.rotate();
        } else {
            rotating.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&rotating.path)
                .ok();
        }

        rotating
    }

    /// Rotates the current log file to `.1` and opens a fresh empty log file.
    pub fn rotate(&mut self) {
        if let Some(mut f) = self.file.take() {
            let _ = f.flush();
        }

        let backup = backup_path(&self.path);
        // On Unix, rename atomically replaces the destination.
        // If rename fails (e.g. permission or target locked), attempt removal first.
        if std::fs::rename(&self.path, &backup).is_err() {
            let _ = std::fs::remove_file(&backup);
            let _ = std::fs::rename(&self.path, &backup);
        }

        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
        {
            Ok(f) => {
                self.file = Some(f);
                self.current_size = 0;
            }
            Err(e) => {
                eprintln!(
                    "Failed to open rotated log file {}: {e}",
                    self.path.display()
                );
                self.file = None;
                self.current_size = 0;
            }
        }
    }

    /// Returns the current size in bytes of the active log file.
    pub fn current_size(&self) -> u64 {
        self.current_size
    }

    /// Returns the path to the active log file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.current_size + (buf.len() as u64) > self.max_file_size {
            self.rotate();
        }

        if let Some(ref mut f) = self.file {
            let written = f.write(buf)?;
            self.current_size += written as u64;
            Ok(written)
        } else {
            // If the file could not be opened, absorb the write rather than panicking
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(ref mut f) = self.file {
            f.flush()
        } else {
            Ok(())
        }
    }
}

/// Writer that streams log records to stdout and a shared `RotatingFile`.
#[derive(Clone)]
pub struct DualWriter {
    rotating: Arc<Mutex<RotatingFile>>,
}

impl DualWriter {
    pub fn new(rotating: Arc<Mutex<RotatingFile>>) -> Self {
        Self { rotating }
    }
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = std::io::stdout().write_all(buf);

        let mut rot = match self.rotating.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _ = rot.write_all(buf);

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = std::io::stdout().flush();

        let mut rot = match self.rotating.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _ = rot.flush();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_rotating_file_rotation_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("test.log");
        let backup_path = dir.path().join("test.log.1");

        // Set max file size to 100 bytes
        let mut rotating = RotatingFile::new(log_path.clone(), 100);
        assert_eq!(rotating.current_size(), 0);

        // Write 60 bytes
        let chunk1 = vec![b'a'; 60];
        rotating.write_all(&chunk1).unwrap();
        rotating.flush().unwrap();
        assert_eq!(rotating.current_size(), 60);
        assert!(!backup_path.exists());

        // Write 50 bytes (60 + 50 = 110 > 100) -> should trigger rotation
        let chunk2 = vec![b'b'; 50];
        rotating.write_all(&chunk2).unwrap();
        rotating.flush().unwrap();
        assert_eq!(rotating.current_size(), 50);

        // Verify backup exists with chunk1's content
        assert!(backup_path.exists());
        assert_eq!(std::fs::metadata(&backup_path).unwrap().len(), 60);
        assert_eq!(std::fs::metadata(&log_path).unwrap().len(), 50);

        // Write another 60 bytes (50 + 60 = 110 > 100) -> triggers second rotation
        let chunk3 = vec![b'c'; 60];
        rotating.write_all(&chunk3).unwrap();
        rotating.flush().unwrap();
        assert_eq!(rotating.current_size(), 60);

        // Verify backup now has chunk2's content (50 bytes)
        assert_eq!(std::fs::metadata(&backup_path).unwrap().len(), 50);
        assert_eq!(std::fs::metadata(&log_path).unwrap().len(), 60);

        // Total disk usage: 50 + 60 = 110 bytes (never piles up indefinitely)
        let total_size = std::fs::metadata(&backup_path).unwrap().len()
            + std::fs::metadata(&log_path).unwrap().len();
        assert!(total_size <= 200);
    }

    #[test]
    fn test_rotating_file_rotates_on_startup_if_oversized() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("startup_test.log");
        let backup_path = dir.path().join("startup_test.log.1");

        // Pre-create an oversized file (150 bytes)
        std::fs::write(&log_path, vec![b'x'; 150]).unwrap();

        // Initialize with max_file_size = 100
        let mut rotating = RotatingFile::new(log_path.clone(), 100);
        assert_eq!(rotating.current_size(), 0);

        // Check that the old file was moved to .1
        assert!(backup_path.exists());
        assert_eq!(std::fs::metadata(&backup_path).unwrap().len(), 150);

        // New writes go to the fresh log
        rotating.write_all(b"fresh data").unwrap();
        rotating.flush().unwrap();
        assert_eq!(rotating.current_size(), 10);
        assert_eq!(std::fs::metadata(&log_path).unwrap().len(), 10);
    }

    #[test]
    fn test_dual_writer_writes_to_rotating_file() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("dual.log");

        let rotating = Arc::new(Mutex::new(RotatingFile::new(log_path.clone(), 500)));
        let mut dual = DualWriter::new(rotating.clone());

        dual.write_all(b"hello dual writer\n").unwrap();
        dual.flush().unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(content, "hello dual writer\n");
    }
}
