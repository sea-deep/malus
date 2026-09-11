//! Process lifecycle supervisor for host browser instances.
//!
//! Features:
//! - Process group leader setup (`setpgid(0, 0)`) so the entire tree of renderer/utility processes belongs to the group
//! - Linux parent-death signaling (`PR_SET_PDEATHSIG`) as best-effort defense against parent crashes
//! - Ephemeral DevTools port allocation and parser for `DevToolsActivePort`
//! - Explicit async shutdown with SIGTERM -> grace timeout -> SIGKILL escalation on the process group
//! - Emergency cleanup in Drop

use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tokio::{process::Child, time::sleep};

use crate::{discovery::BrowserCandidate, error::WebError, profile::ProfileManager};

/// Controls whether the browser window is visible or headless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchMode {
    /// Normal headed browser window.
    #[default]
    Headed,
    /// Headless mode (suitable for automation, tests, and headless servers).
    Headless,
}

/// Status of the managed browser process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    Running { pid: u32, pgid: i32 },
    Exited { exit_code: Option<i32> },
}

pub struct BrowserProcess {
    child: std::sync::Mutex<Option<Child>>,
    pid: u32,
    pgid: i32,
    port: u16,
    browser_ws_url: String,
    port_file: PathBuf,
}

impl BrowserProcess {
    /// Spawn the browser process using the given candidate and profile.
    pub async fn spawn(
        candidate: &BrowserCandidate,
        profile: &ProfileManager,
        launch_mode: LaunchMode,
        initial_url: &str,
        extra_args: &[String],
    ) -> Result<Self, WebError> {
        profile.prepare_profile_dir()?;

        let mut cmd = if candidate.exec_cmd.len() == 1 {
            tokio::process::Command::new(&candidate.exec_cmd[0])
        } else {
            let mut c = tokio::process::Command::new(&candidate.exec_cmd[0]);
            c.args(&candidate.exec_cmd[1..]);
            c
        };

        // Profile isolation
        cmd.arg(format!(
            "--user-data-dir={}",
            profile.profile_dir().display()
        ));

        // Ephemeral DevTools port allocation
        cmd.arg("--remote-debugging-port=0");

        // Standard hygiene flags
        cmd.arg("--no-first-run");
        cmd.arg("--no-default-browser-check");

        // Autoplay & background throttling flags
        cmd.arg("--autoplay-policy=no-user-gesture-required");
        cmd.arg("--disable-background-timer-throttling");
        cmd.arg("--disable-backgrounding-occluded-windows");
        cmd.arg("--disable-renderer-backgrounding");
        cmd.arg(
            "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling,Translate,OptimizationHints,MediaRouter,DialMediaRouteProvider,PaintHolding",
        );

        // Structured LaunchMode
        match launch_mode {
            LaunchMode::Headless => {
                cmd.arg("--headless=new");
                cmd.arg("--disable-gpu");
            }
            LaunchMode::Headed => {
                // Keep headed without forcing headless or suppressing UI
            }
        }

        // Caller-provided extra arguments
        for arg in extra_args {
            cmd.arg(arg);
        }

        // Forward display and audio environment variables
        for var in &[
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "XDG_RUNTIME_DIR",
            "PULSE_SERVER",
            "PIPEWIRE_RUNTIME_DIR",
            "ALSA_CARD",
        ] {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }

        // Target URL
        cmd.arg(initial_url);

        // Detach I/O to avoid terminal corruption
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        // Process group leadership and parent-death signalling (Linux)
        let parent_pid = std::process::id() as libc::pid_t;
        unsafe {
            cmd.pre_exec(move || {
                // Become process group leader: child PID becomes its PGID
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // Request SIGTERM when the Malus parent process exits
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // Check if parent process died during fork
                if libc::getppid() != parent_pid {
                    libc::raise(libc::SIGTERM);
                }
                Ok(())
            });
        }

        let port_file = profile.devtools_port_file();
        // Remove any stale DevToolsActivePort before spawning
        let _ = fs::remove_file(&port_file);

        let mut child = cmd.spawn().map_err(|e| {
            WebError::Launch(format!(
                "Failed to spawn candidate {} (path: {}): {}",
                candidate.display_name,
                candidate.path.display(),
                e
            ))
        })?;

        let pid = child.id().ok_or_else(|| {
            WebError::Launch("Browser process exited immediately after spawn".to_string())
        })?;
        let pgid = pid as i32;

        // Poll DevToolsActivePort to retrieve ephemeral port and browser endpoint path
        let (port, browser_target_path) =
            match wait_for_devtools_port(&mut child, &port_file, Duration::from_secs(12)).await {
                Ok(res) => res,
                Err(e) => {
                    // Clean up spawned process tree on failure
                    unsafe {
                        libc::kill(-pgid, libc::SIGKILL);
                    }
                    let _ = child.wait().await;
                    return Err(e);
                }
            };

        let browser_ws_url = format!("ws://127.0.0.1:{}{}", port, browser_target_path);

        Ok(Self {
            child: std::sync::Mutex::new(Some(child)),
            pid,
            pgid,
            port,
            browser_ws_url,
            port_file,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn pgid(&self) -> i32 {
        self.pgid
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn browser_ws_url(&self) -> &str {
        &self.browser_ws_url
    }

    /// Check the current status of the browser child process.
    pub fn check_status(&self) -> Result<ProcessStatus, WebError> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| WebError::Internal("Process mutex poisoned".into()))?;
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(None) => Ok(ProcessStatus::Running {
                    pid: self.pid,
                    pgid: self.pgid,
                }),
                Ok(Some(status)) => Ok(ProcessStatus::Exited {
                    exit_code: status.code(),
                }),
                Err(e) => Err(WebError::Io(e)),
            }
        } else {
            Ok(ProcessStatus::Exited { exit_code: None })
        }
    }

    /// Explicitly terminate the browser process group with async SIGTERM -> grace -> SIGKILL escalation.
    pub async fn shutdown(&mut self) -> Result<(), WebError> {
        let child_opt = self.child.lock().map(|mut g| g.take()).unwrap_or(None);
        if let Some(mut child) = child_opt {
            let pid = self.pid as i32;
            let pgid = self.pgid;

            // 1. Check if process already exited (e.g. from CDP Browser.close)
            if let Ok(Some(status)) = child.try_wait() {
                tracing::debug!(
                    "Browser process {} already exited with status: {:?}",
                    pid,
                    status
                );
                let _ = fs::remove_file(&self.port_file);
                return Ok(());
            }

            // 2. Wait up to 1.5s for process to finish clean exit
            let grace = tokio::time::timeout(Duration::from_millis(1500), child.wait()).await;
            if let Ok(Ok(status)) = grace {
                tracing::debug!(
                    "Browser process {} exited cleanly with status: {:?}",
                    pid,
                    status
                );
                let _ = fs::remove_file(&self.port_file);
                return Ok(());
            }

            // 3. Send SIGTERM to the main browser process to allow internal SQLite flush
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }

            let grace2 = tokio::time::timeout(Duration::from_millis(1500), child.wait()).await;
            match grace2 {
                Ok(Ok(status)) => {
                    tracing::debug!(
                        "Browser process {} exited after SIGTERM with status: {:?}",
                        pid,
                        status
                    );
                }
                _ => {
                    tracing::warn!(
                        "Browser process group {} did not exit within grace period; sending SIGKILL",
                        pgid
                    );
                    unsafe {
                        libc::kill(-pgid, libc::SIGKILL);
                    }
                    let _ = child.wait().await;
                }
            }

            // Clean up DevToolsActivePort file
            let _ = fs::remove_file(&self.port_file);
        }

        Ok(())
    }
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        let child_opt = self.child.lock().map(|mut g| g.take()).unwrap_or(None);
        if let Some(mut child) = child_opt {
            let pid = self.pid as i32;
            let pgid = self.pgid;
            // Best-effort emergency cleanup in Drop
            if let Ok(None) = child.try_wait() {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                std::thread::sleep(Duration::from_millis(100));
                if let Ok(None) = child.try_wait() {
                    unsafe {
                        libc::kill(-pgid, libc::SIGKILL);
                    }
                    let _ = child.try_wait();
                }
            }
            let _ = fs::remove_file(&self.port_file);
        }
    }
}

/// Poll `<profile>/DevToolsActivePort` until Chromium flushes the port and browser target path.
async fn wait_for_devtools_port(
    child: &mut Child,
    port_file: &Path,
    timeout: Duration,
) -> Result<(u16, String), WebError> {
    let start = tokio::time::Instant::now();

    while start.elapsed() < timeout {
        // Check if child exited prematurely
        if let Ok(Some(status)) = child.try_wait() {
            return Err(WebError::BrowserExited(status.code()));
        }

        if port_file.exists()
            && let Ok(file) = fs::File::open(port_file)
        {
            let mut reader = BufReader::new(file);
            let mut line1 = String::new();
            let mut line2 = String::new();

            if reader.read_line(&mut line1).is_ok() && reader.read_line(&mut line2).is_ok() {
                let port_str = line1.trim();
                let target_path = line2.trim().to_string();

                if let Ok(port) = port_str.parse::<u16>()
                    && port > 0
                    && !target_path.is_empty()
                {
                    return Ok((port, target_path));
                }
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    Err(WebError::PortTimeout(port_file.to_path_buf()))
}
