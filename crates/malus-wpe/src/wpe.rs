//! WPE WebKit backend supervisor and IPC driver.
//!
//! Spawns and manages a `malus-wpe-host` process in `--malus-ipc` mode, communicating
//! over stdin/stdout with JSON-RPC-style request correlation and event sinks.

use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, broadcast, mpsc, oneshot},
};
use tracing::{debug, info, warn};

use crate::{
    discovery::{BrowserEngine, BrowserProduct, WpeCandidate},
    error::WebError,
    page::{PageHealth, WebEvent},
    process::LaunchMode,
    profile::ProfileManager,
    runtime::{RuntimeHealth, RuntimeOptions},
};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, WebError>>>>>;

/// Provider-facing handle for a WPE WebKit page.
pub struct WpePage {
    request_tx: mpsc::Sender<IpcCommand>,
    event_tx: broadcast::Sender<WebEvent>,
    current_url: Arc<Mutex<String>>,
    connected: Arc<AtomicBool>,
    pid: u32,
}

enum IpcCommand {
    Request {
        id: u64,
        method: &'static str,
        params: Value,
        reply_tx: oneshot::Sender<Result<Value, WebError>>,
    },
    Shutdown {
        reply_tx: oneshot::Sender<Result<(), WebError>>,
    },
}

impl WpePage {
    pub async fn navigate(&self, url: &str) -> Result<(), WebError> {
        let (tx, rx) = oneshot::channel();
        let cmd = IpcCommand::Request {
            id: generate_request_id(),
            method: "navigate",
            params: serde_json::json!({ "url": url }),
            reply_tx: tx,
        };
        self.request_tx
            .send(cmd)
            .await
            .map_err(|_| WebError::Disconnected("WPE IPC actor channel closed".into()))?;

        rx.await
            .map_err(|_| WebError::Disconnected("WPE IPC actor dropped request".into()))??;

        *self.current_url.lock().await = url.to_string();
        Ok(())
    }

    pub async fn load_document(&self, url: &str, html: &str) -> Result<(), WebError> {
        let (tx, rx) = oneshot::channel();
        let cmd = IpcCommand::Request {
            id: generate_request_id(),
            method: "load_document",
            params: serde_json::json!({
                "url": url,
                "html": html,
            }),
            reply_tx: tx,
        };
        self.request_tx
            .send(cmd)
            .await
            .map_err(|_| WebError::Disconnected("WPE IPC actor channel closed".into()))?;

        rx.await
            .map_err(|_| WebError::Disconnected("WPE IPC actor dropped request".into()))??;

        *self.current_url.lock().await = url.to_string();
        Ok(())
    }

    pub async fn reload(&self) -> Result<(), WebError> {
        let (tx, rx) = oneshot::channel();
        let cmd = IpcCommand::Request {
            id: generate_request_id(),
            method: "reload",
            params: serde_json::json!({}),
            reply_tx: tx,
        };
        self.request_tx
            .send(cmd)
            .await
            .map_err(|_| WebError::Disconnected("WPE IPC actor channel closed".into()))?;

        rx.await
            .map_err(|_| WebError::Disconnected("WPE IPC actor dropped request".into()))??;
        Ok(())
    }

    pub async fn evaluate(&self, expression: &str) -> Result<Value, WebError> {
        self.evaluate_with_timeout(expression, Duration::from_secs(15))
            .await
    }

    pub async fn evaluate_with_timeout(
        &self,
        expression: &str,
        timeout: Duration,
    ) -> Result<Value, WebError> {
        let (tx, rx) = oneshot::channel();
        let cmd = IpcCommand::Request {
            id: generate_request_id(),
            method: "evaluate",
            params: serde_json::json!({ "expression": expression }),
            reply_tx: tx,
        };
        self.request_tx
            .send(cmd)
            .await
            .map_err(|_| WebError::Disconnected("WPE IPC actor channel closed".into()))?;

        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| WebError::Timeout(format!("evaluate timed out after {:?}", timeout)))?
            .map_err(|_| WebError::Disconnected("WPE IPC actor dropped request".into()))?
    }

    pub async fn call_function(
        &self,
        function_declaration: &str,
        arguments: &[Value],
    ) -> Result<Value, WebError> {
        self.call_function_with_timeout(function_declaration, arguments, Duration::from_secs(30))
            .await
    }

    pub async fn call_function_with_timeout(
        &self,
        function_declaration: &str,
        arguments: &[Value],
        timeout: Duration,
    ) -> Result<Value, WebError> {
        let (tx, rx) = oneshot::channel();
        let cmd = IpcCommand::Request {
            id: generate_request_id(),
            method: "call_function",
            params: serde_json::json!({
                "function": function_declaration,
                "arguments": arguments,
            }),
            reply_tx: tx,
        };
        self.request_tx
            .send(cmd)
            .await
            .map_err(|_| WebError::Disconnected("WPE IPC actor channel closed".into()))?;

        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| WebError::Timeout(format!("call_function timed out after {:?}", timeout)))?
            .map_err(|_| WebError::Disconnected("WPE IPC actor dropped request".into()))?
    }

    pub async fn wait_for_expression(
        &self,
        expression: &str,
        timeout_duration: Duration,
    ) -> Result<Value, WebError> {
        let start = tokio::time::Instant::now();
        let mut last_err = None;

        while start.elapsed() < timeout_duration {
            match self.evaluate(expression).await {
                Ok(val) => {
                    let is_truthy = match &val {
                        Value::Bool(b) => *b,
                        Value::Null => false,
                        Value::String(s) => !s.is_empty(),
                        Value::Number(_) => true,
                        Value::Array(_) | Value::Object(_) => true,
                    };
                    if is_truthy {
                        return Ok(val);
                    }
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(WebError::Timeout(format!(
            "Timed out waiting for expression '{}' (last error: {:?})",
            expression, last_err
        )))
    }

    pub async fn register_event_sink(&self, name: &str) -> Result<(), WebError> {
        let (tx, rx) = oneshot::channel();
        let cmd = IpcCommand::Request {
            id: generate_request_id(),
            method: "register_event_sink",
            params: serde_json::json!({ "name": name }),
            reply_tx: tx,
        };
        self.request_tx
            .send(cmd)
            .await
            .map_err(|_| WebError::Disconnected("WPE IPC actor channel closed".into()))?;

        rx.await
            .map_err(|_| WebError::Disconnected("WPE IPC actor dropped request".into()))??;
        Ok(())
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<WebEvent> {
        self.event_tx.subscribe()
    }

    pub async fn check_health(&self) -> Result<PageHealth, WebError> {
        let url = self.current_url.lock().await.clone();
        let is_connected = self.connected.load(Ordering::SeqCst);
        Ok(PageHealth {
            target_id: format!("wpe-{}", self.pid),
            session_id: format!("wpe-session-{}", self.pid),
            connected: is_connected,
            url,
        })
    }
}

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_request_id() -> u64 {
    REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Internal WPE backend supervisor.
pub struct WpeBackend {
    candidate: WpeCandidate,
    page: Arc<WpePage>,
    pid: u32,
    pgid: i32,
    child: Arc<Mutex<Option<Child>>>,
}

impl WpeBackend {
    pub async fn launch(
        candidate: &WpeCandidate,
        options: RuntimeOptions,
    ) -> Result<Self, WebError> {
        let profile = if let Some(custom) = options.custom_profile_path {
            ProfileManager::with_custom_path(custom)
        } else if let Some(ns) = options.profile_namespace {
            ProfileManager::for_namespace(&ns)?
        } else {
            ProfileManager::for_namespace("default")?
        };

        profile.prepare_profile_dir()?;

        let mut cmd = Command::new(&candidate.binary_path);
        cmd.arg("--malus-ipc");

        if options.launch_mode == LaunchMode::Headless
            || options.launch_mode == LaunchMode::Windowless
        {
            cmd.arg("--headless");
        }

        let cookies_file = profile.profile_dir().join("cookies.sqlite");
        cmd.arg(format!("--cookies-file={}", cookies_file.display()));

        let data_dir = profile.profile_dir().join("data");
        let cache_dir = profile.profile_dir().join("cache");
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = std::fs::create_dir_all(&cache_dir);
        cmd.arg(format!("--data-dir={}", data_dir.display()));
        cmd.arg(format!("--cache-dir={}", cache_dir.display()));

        for arg in &options.extra_args {
            cmd.arg(arg);
        }

        // Minimal required sandbox paths:
        // 1. Profile storage directory (Read-Write)
        cmd.arg(format!(
            "--sandbox-path-rw={}",
            profile.profile_dir().display()
        ));

        // 2. OpenCDM library directory (Read-Only)
        if let Some(ocdm) = &candidate.ocdm_path
            && let Some(parent) = ocdm.parent()
        {
            cmd.arg(format!("--sandbox-path-ro={}", parent.display()));
        }

        // 3. Centralized Widevine CDM discovery and sandbox mount
        let widevine_inst = match crate::widevine::discover_widevine() {
            Ok(inst) => Some(inst),
            Err(e) => {
                debug!("Widevine not available during WPE launch: {e}");
                None
            }
        };

        if let Some(ref widevine) = widevine_inst {
            cmd.arg(format!(
                "--sandbox-path-ro={}",
                widevine.directory.display()
            ));
        }

        // 4. PipeWire socket for audio playback (Read-Write)
        if let Some(pw_sock) = discover_pipewire_socket() {
            cmd.arg(format!("--sandbox-path-rw={}", pw_sock.display()));
        }

        // 5. Executable directory for WPEWebProcess (Read-Only)
        if let Some(exec_path) = &candidate.exec_path {
            cmd.arg(format!("--sandbox-path-ro={}", exec_path.display()));
        }

        // 6. Candidate library paths (Read-Only)
        for p in &candidate.library_paths {
            cmd.arg(format!("--sandbox-path-ro={}", p.display()));
        }

        cmd.arg(&options.initial_url);

        // Configure environment
        let mut ld_paths = Vec::new();
        for p in &candidate.library_paths {
            ld_paths.push(p.to_string_lossy().to_string());
        }
        if let Ok(existing_ld) = std::env::var("LD_LIBRARY_PATH") {
            ld_paths.push(existing_ld);
        }
        cmd.env("LD_LIBRARY_PATH", ld_paths.join(":"));

        if let Some(exec_path) = &candidate.exec_path {
            cmd.env("WEBKIT_EXEC_PATH", exec_path);
        }

        if let Some(ref widevine) = widevine_inst {
            cmd.env("MALUS_WIDEVINE_PATH", &widevine.library_path);
        } else {
            cmd.env_remove("MALUS_WIDEVINE_PATH");
        }

        // Process group and parent-death signal configuration
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        info!(
            "Spawning malus-wpe-host: {} (headless: {})",
            candidate.binary_path.display(),
            options.launch_mode == LaunchMode::Headless
        );

        let mut child = cmd.spawn().map_err(|e| WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: format!("Failed to spawn malus-wpe-host: {e}"),
        })?;

        let pid = child.id().ok_or_else(|| WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: "Failed to obtain malus-wpe-host process ID".into(),
        })?;
        let pgid = pid as i32;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| WebError::Initialization {
                engine: BrowserEngine::Wpe,
                message: "Failed to capture malus-wpe-host stdout".into(),
            })?;
        let mut stdin = child.stdin.take().ok_or_else(|| WebError::Initialization {
            engine: BrowserEngine::Wpe,
            message: "Failed to capture malus-wpe-host stdin".into(),
        })?;
        let stderr = child.stderr.take();
        let captured_stderr = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured_for_task = captured_stderr.clone();

        // Background drain for stderr to log and capture errors
        if let Some(err_pipe) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(err_pipe).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    tracing::warn!(target: "wpe_stderr", "[malus-wpe-host] {}", line);
                    let mut guard = captured_for_task.lock().unwrap();
                    if guard.len() < 50 {
                        guard.push(line);
                    }
                }
            });
        }

        // Handshake: read first line to confirm malus-wpe-host ready event
        let mut stdout_reader = BufReader::new(stdout);
        let mut first_line = String::new();

        let handshake_res = tokio::time::timeout(
            Duration::from_secs(10),
            stdout_reader.read_line(&mut first_line),
        )
        .await;

        match handshake_res {
            Ok(Ok(n)) if n > 0 => {
                debug!("WPE handshake received: {}", first_line.trim());
                if let Ok(v) = serde_json::from_str::<Value>(&first_line)
                    && v.get("event").and_then(|e| e.as_str()) != Some("ready")
                {
                    warn!(
                        "WPE first message was not ready event: {}",
                        first_line.trim()
                    );
                }
            }
            Ok(Ok(_)) => {
                kill_pgid(pgid);
                let err_details = captured_stderr.lock().unwrap().join("; ");
                return Err(WebError::Initialization {
                    engine: BrowserEngine::Wpe,
                    message: format!(
                        "malus-wpe-host closed stdout immediately during startup (stderr: {err_details})"
                    ),
                });
            }
            Ok(Err(e)) => {
                kill_pgid(pgid);
                return Err(WebError::Initialization {
                    engine: BrowserEngine::Wpe,
                    message: format!("I/O error reading malus-wpe-host handshake: {e}"),
                });
            }
            Err(_) => {
                kill_pgid(pgid);
                return Err(WebError::Initialization {
                    engine: BrowserEngine::Wpe,
                    message: "Timed out waiting for malus-wpe-host ready handshake".into(),
                });
            }
        }

        let pending_requests: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, _) = broadcast::channel::<WebEvent>(256);
        let (request_tx, mut request_rx) = mpsc::channel::<IpcCommand>(128);

        let connected = Arc::new(AtomicBool::new(true));
        let current_url = Arc::new(Mutex::new(options.initial_url.clone()));

        // Spawn background reader task for stdout lines
        let pending_for_reader = pending_requests.clone();
        let event_tx_for_reader = event_tx.clone();
        let connected_for_reader = connected.clone();

        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match stdout_reader.read_line(&mut line).await {
                    Ok(0) => {
                        debug!("malus-wpe-host stdout reached EOF");
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                            if let Some(event_name) = val.get("event").and_then(|e| e.as_str()) {
                                let raw_payload = val.get("payload");
                                let payload = match raw_payload {
                                    Some(Value::String(s)) => serde_json::from_str(s)
                                        .unwrap_or_else(|_| Value::String(s.clone())),
                                    Some(v) => v.clone(),
                                    None => Value::Null,
                                };
                                let _ = event_tx_for_reader.send(WebEvent {
                                    name: event_name.to_string(),
                                    payload,
                                });
                            } else if let Some(id) = val.get("id").and_then(|i| i.as_u64()) {
                                let mut guard = pending_for_reader.lock().await;
                                if let Some(reply_tx) = guard.remove(&id) {
                                    if let Some(err) = val.get("error") {
                                        let err_msg =
                                            err.as_str().unwrap_or("Unknown script error");
                                        let _ = reply_tx
                                            .send(Err(WebError::Evaluation(err_msg.to_string())));
                                    } else {
                                        let res = val.get("result").cloned().unwrap_or(Value::Null);
                                        let _ = reply_tx.send(Ok(res));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error reading malus-wpe-host stdout: {e}");
                        break;
                    }
                }
            }

            connected_for_reader.store(false, Ordering::SeqCst);
            // Fail any remaining pending requests
            let mut guard = pending_for_reader.lock().await;
            for (_, tx) in guard.drain() {
                let _ = tx.send(Err(WebError::Disconnected(
                    "WPE process disconnected".into(),
                )));
            }
        });

        // Spawn background writer task for stdin commands
        let pending_for_writer = pending_requests.clone();
        let connected_for_writer = connected.clone();

        tokio::spawn(async move {
            while let Some(cmd) = request_rx.recv().await {
                match cmd {
                    IpcCommand::Request {
                        id,
                        method,
                        params,
                        reply_tx,
                    } => {
                        let req_obj = serde_json::json!({
                            "id": id,
                            "method": method,
                            "params": params,
                        });
                        let mut line = req_obj.to_string();
                        line.push('\n');

                        {
                            let mut guard = pending_for_writer.lock().await;
                            guard.insert(id, reply_tx);
                        }

                        if let Err(e) = stdin.write_all(line.as_bytes()).await {
                            warn!("Failed to write to malus-wpe-host stdin: {e}");
                            let mut guard = pending_for_writer.lock().await;
                            if let Some(tx) = guard.remove(&id) {
                                let _ = tx.send(Err(WebError::Disconnected(format!(
                                    "Failed to write to stdin: {e}"
                                ))));
                            }
                            connected_for_writer.store(false, Ordering::SeqCst);
                            break;
                        }
                        let _ = stdin.flush().await;
                    }
                    IpcCommand::Shutdown { reply_tx } => {
                        let shutdown_obj = serde_json::json!({
                            "id": 0,
                            "method": "shutdown",
                            "params": {},
                        });
                        let mut line = shutdown_obj.to_string();
                        line.push('\n');
                        let _ = stdin.write_all(line.as_bytes()).await;
                        let _ = stdin.flush().await;
                        let _ = reply_tx.send(Ok(()));
                        break;
                    }
                }
            }
        });

        let page = Arc::new(WpePage {
            request_tx,
            event_tx,
            current_url,
            connected,
            pid,
        });

        Ok(Self {
            candidate: candidate.clone(),
            page,
            pid,
            pgid,
            child: Arc::new(Mutex::new(Some(child))),
        })
    }

    pub fn page(&self) -> &Arc<WpePage> {
        &self.page
    }

    pub fn candidate(&self) -> &WpeCandidate {
        &self.candidate
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub async fn check_health(&self) -> Result<RuntimeHealth, WebError> {
        let is_alive = is_pid_alive(self.pid as i32);
        let page_health = self.page.check_health().await?;

        let details = if is_alive && page_health.connected {
            format!("Running (PID: {}, PGID: {})", self.pid, self.pgid)
        } else if is_alive {
            "Process running but IPC disconnected".to_string()
        } else {
            "Process terminated".to_string()
        };

        Ok(RuntimeHealth {
            alive: is_alive && page_health.connected,
            engine: BrowserEngine::Wpe,
            product: BrowserProduct::WpeWebKit,
            pid: self.pid,
            port: None,
            page: page_health,
            details,
        })
    }

    pub async fn shutdown(self) -> Result<(), WebError> {
        // Send shutdown request via IPC
        let (tx, rx) = oneshot::channel();
        let _ = self
            .page
            .request_tx
            .send(IpcCommand::Shutdown { reply_tx: tx })
            .await;
        let _ = tokio::time::timeout(Duration::from_millis(1500), rx).await;

        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            // Wait for clean exit
            let wait_res = tokio::time::timeout(Duration::from_millis(1500), child.wait()).await;
            if wait_res.is_err() {
                // Escalate to SIGTERM
                warn!(
                    "malus-wpe-host did not exit cleanly; sending SIGTERM to PGID {}",
                    self.pgid
                );
                kill_pgid_signal(self.pgid, libc::SIGTERM);

                let term_res =
                    tokio::time::timeout(Duration::from_millis(1500), child.wait()).await;
                if term_res.is_err() {
                    warn!(
                        "malus-wpe-host did not terminate; sending SIGKILL to PGID {}",
                        self.pgid
                    );
                    kill_pgid_signal(self.pgid, libc::SIGKILL);
                    let _ = child.wait().await;
                }
            }
        }

        Ok(())
    }
}

impl Drop for WpeBackend {
    fn drop(&mut self) {
        if is_pid_alive(self.pid as i32) {
            kill_pgid_signal(self.pgid, libc::SIGKILL);
        }
    }
}

fn is_pid_alive(pid: i32) -> bool {
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

fn kill_pgid(pgid: i32) {
    kill_pgid_signal(pgid, libc::SIGTERM);
    std::thread::sleep(Duration::from_millis(100));
    if is_pid_alive(pgid) {
        kill_pgid_signal(pgid, libc::SIGKILL);
    }
}

fn kill_pgid_signal(pgid: i32, sig: i32) {
    if pgid <= 0 {
        return;
    }
    unsafe {
        libc::kill(-pgid, sig);
    }
}

fn discover_pipewire_socket() -> Option<PathBuf> {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let sock = PathBuf::from(runtime_dir).join("pipewire-0");
        if sock.exists() {
            return Some(sock);
        }
    }
    let uid = unsafe { libc::getuid() };
    let fallback_sock = PathBuf::from(format!("/run/user/{uid}/pipewire-0"));
    if fallback_sock.exists() {
        return Some(fallback_sock);
    }
    None
}
