use std::path::PathBuf;

/// Determine the default socket path for malusd.
pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("MALUS_SOCKET") {
        return PathBuf::from(path);
    }
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("malus.sock");
    }
    PathBuf::from("/tmp/malus.sock")
}
