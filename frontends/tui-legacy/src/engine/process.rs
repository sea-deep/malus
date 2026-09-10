//! Browser process lifecycle supervisor for Malus on Linux.
//!
//! Enforces zero-zombie guarantees via `PR_SET_PDEATHSIG`, applies anti-throttling
//! and security flags, and reads the ephemeral DevTools port.

use std::{
    env, fs,
    io::{self, BufRead, BufReader, ErrorKind},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::discovery::BrowserCandidate;
use super::profile::ProfileManager;

pub struct BrowserProcess {
    child: Option<Child>,
    pub port: u16,
    pub browser_target_path: String,
    pub profile: ProfileManager,
}

impl BrowserProcess {
    /// Launch the selected browser in headless mode or visible mode.
    pub fn spawn(
        candidate: &BrowserCandidate,
        profile: ProfileManager,
        visible: bool,
        initial_url: &str,
    ) -> io::Result<Self> {
        // Ensure profile directory is securely initialized
        profile.prepare_profile_dir()?;

        let mut cmd = if candidate.exec_cmd.len() == 1 {
            Command::new(&candidate.exec_cmd[0])
        } else {
            let mut c = Command::new(&candidate.exec_cmd[0]);
            c.args(&candidate.exec_cmd[1..]);
            c
        };

        // User Data Directory
        cmd.arg(format!(
            "--user-data-dir={}",
            profile.profile_dir().display()
        ));

        // Ephemeral CDP port allocation
        cmd.arg("--remote-debugging-port=0");

        // Headless vs Visible mode
        if !visible {
            cmd.arg("--headless=new");
            cmd.arg("--disable-gpu");
            // Footprint reduction: shrink virtual viewport & suppress software rasterization
            cmd.arg("--window-size=100,100");
            cmd.arg("--disable-gpu-compositing");
            cmd.arg("--disable-software-rasterizer");
            cmd.arg("--disable-gl-drawing-for-tests");
            // Suppress DOM image bitmap decoding in headless mode (Malus downloads artwork directly)
            cmd.arg("--blink-settings=imagesEnabled=false");
            // Clamp V8 heap for headless playback
            cmd.arg("--js-flags=--max-old-space-size=128");
            // Clamp renderer process count
            cmd.arg("--renderer-process-limit=1");
            // Bound disk & media caches
            cmd.arg("--disk-cache-size=10485760");
            cmd.arg("--media-cache-size=20971520");
            // Disable background network tasks
            cmd.arg("--disable-background-networking");
        } else {
            // Visible mode (for Apple ID login)
            cmd.arg(format!("--app={}", initial_url));
        }

        // Anti-throttling & Background Audio Flags
        cmd.arg("--autoplay-policy=no-user-gesture-required");
        cmd.arg("--disable-background-timer-throttling");
        cmd.arg("--disable-backgrounding-occluded-windows");
        cmd.arg("--disable-renderer-backgrounding");
        cmd.arg(
            "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling,Translate,OptimizationHints,MediaRouter,DialMediaRouteProvider,PaintHolding",
        );

        // Security & Hardening Flags (OWASP A05)
        cmd.arg("--deny-permission-prompts");
        cmd.arg("--disable-file-upload");
        cmd.arg("--disable-default-apps");
        cmd.arg("--no-first-run");
        cmd.arg("--no-default-browser-check");
        cmd.arg("--password-store=basic");

        // PipeWire / PulseAudio Linux Audio sink flags
        cmd.arg("--alsa-output-device=default");

        // Forward audio and display environment variables
        for var in &[
            "XDG_RUNTIME_DIR",
            "PULSE_SERVER",
            "PIPEWIRE_RUNTIME_DIR",
            "ALSA_CARD",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
        ] {
            if let Ok(val) = env::var(var) {
                cmd.env(var, val);
            }
        }

        // Target URL
        cmd.arg(initial_url);

        // Detach I/O to avoid polluting terminal, but capture early crashes
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        // Zero-Zombie Guarantee (OWASP / System Reliability):
        // Deliver SIGTERM to the browser child process if Malus dies or panics.
        let parent_pid = std::process::id() as libc::pid_t;
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() != parent_pid {
                    libc::raise(libc::SIGTERM);
                }
                Ok(())
            });
        }

        let port_file = profile.devtools_port_file();
        // Remove stale DevToolsActivePort from previous run to guarantee fresh port reading
        let _ = fs::remove_file(&port_file);

        let mut child = cmd.spawn()?;

        // Poll DevToolsActivePort to discover the freshly assigned port and browser target
        let (port, browser_target_path) =
            match wait_for_devtools_port(&mut child, &port_file, Duration::from_secs(12)) {
                Ok(res) => res,
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e);
                }
            };

        Ok(Self {
            child: Some(child),
            port,
            browser_target_path,
            profile,
        })
    }

    /// Return the OS process ID of the browser child.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Check if the browser process is still running.
    pub fn is_alive(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// Explicitly terminate the browser process gracefully.
    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id() as i32;
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(2) {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Poll `<profile>/DevToolsActivePort` until Chromium flushes the port and target path.
fn wait_for_devtools_port(
    child: &mut Child,
    port_file: &Path,
    timeout: Duration,
) -> io::Result<(u16, String)> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        // Check if child exited prematurely (e.g. crash / missing binary / invalid flag)
        if let Ok(Some(status)) = child.try_wait() {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                format!(
                    "Browser process exited prematurely with status: {:?}",
                    status
                ),
            ));
        }

        if port_file.exists()
            && let Ok(file) = fs::File::open(port_file)
        {
            let mut reader = BufReader::new(file);
            let mut line1 = String::new();
            let mut line2 = String::new();

            if reader.read_line(&mut line1).is_ok() && reader.read_line(&mut line2).is_ok() {
                let port_str = line1.trim();
                let target = line2.trim().to_string();

                if let Ok(port) = port_str.parse::<u16>()
                    && port > 0
                    && !target.is_empty()
                {
                    return Ok((port, target));
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(io::Error::new(
        ErrorKind::TimedOut,
        format!(
            "Timed out waiting for DevToolsActivePort at {}",
            port_file.display()
        ),
    ))
}
