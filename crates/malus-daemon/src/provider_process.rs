//! Process-isolated provider client and supervisor communicating via LSP-framed JSON over standard I/O.
//!
//! Enforces strict physical process boundary, lifecycle state machine, and shutdown escalation:
//! - Child process stdout: strictly protocol bytes (Content-Length framed)
//! - Child process stderr: strictly logs forwarded to tracing
//! - Fault isolation: child crashes or malformed output do not crash the daemon
//! - State machine: Stopped -> Starting -> Handshaking -> Ready / NeedsAuth / Degraded / Restarting / Crashed / Incompatible
//! - Exponential backoff retry policy for unexpected exits
//! - Graceful shutdown escalation: ProviderRequest::Shutdown -> SIGTERM -> SIGKILL

use malus_protocol::{
    DEFAULT_MAX_PAYLOAD_BYTES,
    codec::{FrameError, decode_message, read_frame, write_message},
    provider::{
        PROVIDER_PROTOCOL, ProviderEvent, ProviderRequest, ProviderRequestEnvelope,
        ProviderResponse, ProviderWireMessage,
    },
    wire::{ActionRequestV0, AuthStatusWire, MediaIdWire, PlayerStatusWire, QueueWire, TrackWire},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, mpsc, oneshot},
    time::{sleep, timeout},
};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderSupervisorState {
    Stopped,
    Starting,
    Handshaking,
    Ready,
    NeedsAuth { message: Option<String> },
    Degraded { reason: String },
    Restarting { attempt: u32, next_retry_ms: u64 },
    Crashed { reason: String },
    Incompatible { reason: String },
}

impl fmt::Display for ProviderSupervisorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Handshaking => write!(f, "handshaking"),
            Self::Ready => write!(f, "ready"),
            Self::NeedsAuth { message } => match message {
                Some(msg) => write!(f, "needs_auth: {msg}"),
                None => write!(f, "needs_auth"),
            },
            Self::Degraded { reason } => write!(f, "degraded: {reason}"),
            Self::Restarting {
                attempt,
                next_retry_ms,
            } => {
                write!(
                    f,
                    "restarting (attempt {attempt}, backoff {next_retry_ms}ms)"
                )
            }
            Self::Crashed { reason } => write!(f, "crashed: {reason}"),
            Self::Incompatible { reason } => write!(f, "incompatible: {reason}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SupervisorPolicy {
    pub max_retries: u32,
    pub base_backoff_ms: u64,
    pub handshake_timeout: Duration,
    pub shutdown_grace_period: Duration,
    pub shutdown_kill_timeout: Duration,
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_backoff_ms: 100,
            handshake_timeout: Duration::from_secs(5),
            shutdown_grace_period: Duration::from_millis(1000),
            shutdown_kill_timeout: Duration::from_millis(1000),
        }
    }
}

#[derive(Debug, Error)]
pub enum ProviderProcessError {
    #[error("Failed to spawn provider process: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("Child process exited: {0:?}")]
    ProcessExited(Option<std::process::ExitStatus>),
    #[error("Framing / Protocol error: {0}")]
    Protocol(#[from] FrameError),
    #[error("Provider returned error: [{code}] {message}")]
    ServerError { code: String, message: String },
    #[error("Unexpected response: {0:?}")]
    UnexpectedResponse(Box<ProviderResponse>),
    #[error("Provider is not in a ready state: current state is {0}")]
    NotReady(ProviderSupervisorState),
    #[error("Handshake timed out after {0:?}")]
    HandshakeTimeout(Duration),
}

struct ChildHandle {
    child: Child,
    stdin_tx: mpsc::Sender<ProviderRequestEnvelope>,
    pid: Option<u32>,
    reader_task: tokio::task::JoinHandle<()>,
    writer_task: tokio::task::JoinHandle<()>,
}

pub type ProviderEventCallback = Arc<dyn Fn(ProviderEvent) + Send + Sync>;

pub struct ProviderProcess {
    id: String,
    name: String,
    executable: PathBuf,
    args: Vec<String>,
    policy: SupervisorPolicy,
    state: Arc<Mutex<ProviderSupervisorState>>,
    capabilities: Arc<Mutex<Vec<String>>>,
    retry_count: Arc<Mutex<u32>>,
    handle: Arc<Mutex<Option<ChildHandle>>>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<ProviderResponse>>>>,
    next_request_id: Arc<AtomicU64>,
    event_callback: Arc<Mutex<Option<ProviderEventCallback>>>,
}

impl ProviderProcess {
    /// Spawn and supervise a provider executable child process.
    pub async fn spawn(
        id: impl Into<String>,
        name: impl Into<String>,
        executable: impl AsRef<Path>,
        args: &[&str],
    ) -> Result<Self, ProviderProcessError> {
        Self::spawn_with_policy(id, name, executable, args, SupervisorPolicy::default()).await
    }

    /// Spawn and supervise a provider with a specific supervisor policy.
    pub async fn spawn_with_policy(
        id: impl Into<String>,
        name: impl Into<String>,
        executable: impl AsRef<Path>,
        args: &[&str],
        policy: SupervisorPolicy,
    ) -> Result<Self, ProviderProcessError> {
        let id = id.into();
        let name = name.into();
        let executable = executable.as_ref().to_path_buf();
        let args = args.iter().map(|s| s.to_string()).collect();

        let proc = Self {
            id,
            name,
            executable,
            args,
            policy,
            state: Arc::new(Mutex::new(ProviderSupervisorState::Stopped)),
            capabilities: Arc::new(Mutex::new(Vec::new())),
            retry_count: Arc::new(Mutex::new(0)),
            handle: Arc::new(Mutex::new(None)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            next_request_id: Arc::new(AtomicU64::new(1)),
            event_callback: Arc::new(Mutex::new(None)),
        };

        proc.start_process().await?;
        Ok(proc)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn state(&self) -> ProviderSupervisorState {
        self.state.lock().await.clone()
    }

    pub async fn capabilities(&self) -> Vec<String> {
        self.capabilities.lock().await.clone()
    }

    pub async fn is_ready(&self) -> bool {
        matches!(*self.state.lock().await, ProviderSupervisorState::Ready)
    }

    pub async fn set_event_callback(&self, cb: ProviderEventCallback) {
        *self.event_callback.lock().await = Some(cb);
    }

    /// Internal method to spawn the child process and perform handshake.
    async fn start_process(&self) -> Result<(), ProviderProcessError> {
        *self.state.lock().await = ProviderSupervisorState::Starting;
        debug!(
            "Spawning provider process '{}' from {}",
            self.id,
            self.executable.display()
        );

        let mut child = Command::new(&self.executable)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = child.id();
        let mut stdin = child.stdin.take().expect("Child stdin was piped");
        let mut stdout = child.stdout.take().expect("Child stdout was piped");
        let stderr = child.stderr.take().expect("Child stderr was piped");

        // Forward child stderr to daemon tracing logs
        let provider_id = self.id.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                info!(target: "provider_stderr", "[provider:{}] {}", provider_id, line);
            }
        });

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<ProviderRequestEnvelope>(64);
        let pending_for_writer = self.pending_requests.clone();
        let writer_task = tokio::spawn(async move {
            while let Some(req_env) = stdin_rx.recv().await {
                if let Err(e) = write_message(&mut stdin, &req_env).await {
                    debug!("Failed to write to provider stdin: {e}");
                    let mut map = pending_for_writer.lock().await;
                    map.remove(&req_env.id);
                    break;
                }
            }
        });

        let pending = self.pending_requests.clone();
        let event_cb = self.event_callback.clone();
        let provider_id_for_reader = self.id.clone();
        let reader_task = tokio::spawn(async move {
            let mut read_buf = Vec::new();
            loop {
                match read_frame(&mut stdout, &mut read_buf, DEFAULT_MAX_PAYLOAD_BYTES).await {
                    Ok(Some(frame)) => match decode_message::<ProviderWireMessage>(&frame) {
                        Ok(ProviderWireMessage::Response { id, response }) => {
                            let mut map = pending.lock().await;
                            if let Some(tx) = map.remove(&id) {
                                let _ = tx.send(response);
                            }
                        }
                        Ok(ProviderWireMessage::Event { event }) => {
                            let cb = {
                                let guard = event_cb.lock().await;
                                guard.clone()
                            };
                            if let Some(cb) = cb {
                                cb(event);
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Malformed message from provider '{}': {e}",
                                provider_id_for_reader
                            );
                        }
                    },
                    Ok(None) => {
                        debug!("Provider '{}' stdout reached EOF", provider_id_for_reader);
                        let mut map = pending.lock().await;
                        map.clear();
                        break;
                    }
                    Err(e) => {
                        warn!(
                            "Error reading provider '{}' stdout: {e}",
                            provider_id_for_reader
                        );
                        let mut map = pending.lock().await;
                        map.clear();
                        break;
                    }
                }
            }
        });

        let handle = ChildHandle {
            child,
            stdin_tx,
            pid,
            reader_task,
            writer_task,
        };

        *self.handle.lock().await = Some(handle);
        *self.state.lock().await = ProviderSupervisorState::Handshaking;

        // Perform handshake with timeout
        let handshake_fut = self.send_request(ProviderRequest::Hello {
            version: PROVIDER_PROTOCOL,
        });

        match timeout(self.policy.handshake_timeout, handshake_fut).await {
            Ok(Ok(ProviderResponse::Hello {
                version,
                capabilities,
                status,
                ..
            })) => {
                if version.0 != PROVIDER_PROTOCOL.0 {
                    let reason = format!(
                        "Incompatible protocol version: daemon is {}.{}, provider is {}.{}",
                        PROVIDER_PROTOCOL.0, PROVIDER_PROTOCOL.1, version.0, version.1
                    );
                    warn!("Provider '{}' handshake failed: {reason}", self.id);
                    *self.state.lock().await = ProviderSupervisorState::Incompatible { reason };
                    return Ok(());
                }

                *self.capabilities.lock().await = capabilities.clone();
                *self.retry_count.lock().await = 0;

                if status == "needs_auth" {
                    *self.state.lock().await = ProviderSupervisorState::NeedsAuth { message: None };
                } else {
                    *self.state.lock().await = ProviderSupervisorState::Ready;
                }

                info!(
                    "Provider '{}' registered successfully (state: {}, capabilities: {:?})",
                    self.id,
                    self.state().await,
                    capabilities
                );
                Ok(())
            }
            Ok(Ok(ProviderResponse::Capabilities(caps))) => {
                *self.capabilities.lock().await = caps.clone();
                *self.retry_count.lock().await = 0;
                *self.state.lock().await = ProviderSupervisorState::Ready;
                Ok(())
            }
            Ok(Ok(other)) => {
                let reason = format!("Unexpected handshake response: {other:?}");
                warn!("Provider '{}': {reason}", self.id);
                *self.state.lock().await = ProviderSupervisorState::Degraded {
                    reason: reason.clone(),
                };
                Err(ProviderProcessError::UnexpectedResponse(Box::new(other)))
            }
            Ok(Err(e)) => {
                warn!("Provider '{}' handshake error: {e}", self.id);
                *self.state.lock().await = ProviderSupervisorState::Degraded {
                    reason: e.to_string(),
                };
                Err(e)
            }
            Err(_) => {
                let reason = format!(
                    "Handshake timed out after {:?}",
                    self.policy.handshake_timeout
                );
                warn!("Provider '{}': {reason}", self.id);
                *self.state.lock().await = ProviderSupervisorState::Degraded {
                    reason: reason.clone(),
                };
                Err(ProviderProcessError::HandshakeTimeout(
                    self.policy.handshake_timeout,
                ))
            }
        }
    }

    /// Send a request to the provider child process and read its response.
    pub async fn send_request(
        &self,
        req: ProviderRequest,
    ) -> Result<ProviderResponse, ProviderProcessError> {
        let id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending_requests.lock().await;
            map.insert(id, tx);
        }

        let (stdin_tx, is_dead) = {
            let guard = self.handle.lock().await;
            match guard.as_ref() {
                Some(h) => (Some(h.stdin_tx.clone()), h.reader_task.is_finished()),
                None => (None, false),
            }
        };

        let stdin_tx = match stdin_tx {
            Some(tx) => tx,
            None => {
                self.pending_requests.lock().await.remove(&id);
                return Err(ProviderProcessError::NotReady(self.state().await));
            }
        };

        if is_dead {
            self.pending_requests.lock().await.remove(&id);
            let status = {
                let mut guard = self.handle.lock().await;
                guard
                    .as_mut()
                    .and_then(|h| h.child.try_wait().ok().flatten())
            };
            self.handle_unexpected_exit(status).await;
            return Err(ProviderProcessError::ProcessExited(status));
        }

        let req_env = ProviderRequestEnvelope::new(id, req);
        if stdin_tx.send(req_env).await.is_err() {
            self.pending_requests.lock().await.remove(&id);
            self.handle_unexpected_exit(None).await;
            return Err(ProviderProcessError::ProcessExited(None));
        }

        let resp = match timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.pending_requests.lock().await.remove(&id);
                let status = {
                    let mut guard = self.handle.lock().await;
                    guard
                        .as_mut()
                        .and_then(|h| h.child.try_wait().ok().flatten())
                };
                self.handle_unexpected_exit(status).await;
                return Err(ProviderProcessError::ProcessExited(status));
            }
            Err(_) => {
                // Timeout: remove waiter so it does not leak
                self.pending_requests.lock().await.remove(&id);
                return Err(ProviderProcessError::HandshakeTimeout(Duration::from_secs(
                    30,
                )));
            }
        };

        if let ProviderResponse::Error { code, message } = resp {
            Err(ProviderProcessError::ServerError { code, message })
        } else {
            Ok(resp)
        }
    }

    /// Handle unexpected process exit and apply exponential backoff retry policy.
    async fn handle_unexpected_exit(&self, status: Option<std::process::ExitStatus>) {
        let mut state_guard = self.state.lock().await;
        if *state_guard == ProviderSupervisorState::Stopped {
            return;
        }

        let mut retries = self.retry_count.lock().await;
        if *retries < self.policy.max_retries {
            *retries += 1;
            let attempt = *retries;
            let backoff_ms = self.policy.base_backoff_ms * (1 << (attempt - 1));
            warn!(
                "Provider '{}' exited unexpectedly ({status:?}). Entering restart backoff (attempt {attempt}/{}, wait {}ms)",
                self.id, self.policy.max_retries, backoff_ms
            );
            *state_guard = ProviderSupervisorState::Restarting {
                attempt,
                next_retry_ms: backoff_ms,
            };
        } else {
            let reason = format!(
                "Exited unexpectedly ({status:?}) and exhausted max retries ({})",
                self.policy.max_retries
            );
            error!("Provider '{}' crashed: {reason}", self.id);
            *state_guard = ProviderSupervisorState::Crashed { reason };
        }
    }

    /// Explicitly restart the provider, respecting the supervisor backoff state if active.
    pub async fn restart(&self) -> Result<(), ProviderProcessError> {
        let cur_state = self.state().await;
        if let ProviderSupervisorState::Restarting { next_retry_ms, .. } = cur_state {
            debug!(
                "Sleeping for {}ms backoff before restarting provider '{}'",
                next_retry_ms, self.id
            );
            sleep(Duration::from_millis(next_retry_ms)).await;
        }

        // Clean up previous process
        self.cleanup_child().await;
        self.start_process().await
    }

    /// Graceful shutdown with escalation: Shutdown RPC -> SIGTERM -> SIGKILL.
    pub async fn shutdown(&self) -> Result<(), ProviderProcessError> {
        *self.state.lock().await = ProviderSupervisorState::Stopped;
        self.cleanup_child().await;
        Ok(())
    }

    /// Force kill child process without graceful notification (for fault injection testing).
    pub async fn force_kill_for_test(&self) {
        let mut guard = self.handle.lock().await;
        if let Some(ref mut handle) = *guard {
            let _ = handle.child.start_kill();
        }
    }

    async fn cleanup_child(&self) {
        let mut guard = self.handle.lock().await;
        if let Some(mut handle) = guard.take() {
            debug!("Initiating graceful shutdown for provider '{}'", self.id);

            // 1. Try sending ProviderRequest::Shutdown RPC
            let shutdown_env = ProviderRequestEnvelope::new(0, ProviderRequest::Shutdown);
            let _ = handle.stdin_tx.send(shutdown_env).await;

            // 2. Wait up to shutdown_grace_period for child to exit
            let wait_exit = async {
                let _ = handle.child.wait().await;
            };

            if timeout(self.policy.shutdown_grace_period, wait_exit)
                .await
                .is_err()
            {
                // Grace period expired: send SIGTERM
                if let Some(pid) = handle.pid {
                    warn!(
                        "Provider '{}' did not exit within grace period. Sending SIGTERM to PID {}",
                        self.id, pid
                    );
                    #[cfg(unix)]
                    unsafe {
                        libc::kill(pid as libc::pid_t, libc::SIGTERM);
                    }
                }

                // 3. Wait up to shutdown_kill_timeout for child to exit after SIGTERM
                let wait_sigterm = async {
                    let _ = handle.child.wait().await;
                };

                if timeout(self.policy.shutdown_kill_timeout, wait_sigterm)
                    .await
                    .is_err()
                {
                    // Kill timeout expired: send SIGKILL
                    if let Some(pid) = handle.pid {
                        error!(
                            "Provider '{}' did not terminate after SIGTERM. Sending SIGKILL to PID {}",
                            self.id, pid
                        );
                        #[cfg(unix)]
                        unsafe {
                            libc::kill(pid as libc::pid_t, libc::SIGKILL);
                        }
                    }
                    let _ = handle.child.wait().await;
                }
            }

            handle.reader_task.abort();
            handle.writer_task.abort();
        }

        let mut pending = self.pending_requests.lock().await;
        pending.clear();
    }

    pub async fn search(&self, query: &str) -> Result<Vec<TrackWire>, ProviderProcessError> {
        let resp = self
            .send_request(ProviderRequest::Search {
                query: query.to_string(),
                limit: 20,
            })
            .await?;
        if let ProviderResponse::SearchResults { tracks } = resp {
            Ok(tracks)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn play_track(&self, media_id: &str) -> Result<(), ProviderProcessError> {
        let resp = self
            .send_request(ProviderRequest::PlayTrack {
                media_id: media_id.to_string(),
            })
            .await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn resume(&self) -> Result<(), ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::Play).await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn pause(&self) -> Result<(), ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::Pause).await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn toggle_play(&self) -> Result<(), ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::TogglePlay).await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn stop(&self) -> Result<(), ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::Stop).await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn next(&self) -> Result<(), ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::Next).await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn previous(&self) -> Result<(), ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::Previous).await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn seek(&self, position_ms: u64) -> Result<(), ProviderProcessError> {
        let resp = self
            .send_request(ProviderRequest::Seek { position_ms })
            .await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn set_volume(&self, volume: u8) -> Result<(), ProviderProcessError> {
        let resp = self
            .send_request(ProviderRequest::SetVolume { volume })
            .await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn get_status(&self) -> Result<PlayerStatusWire, ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::GetStatus).await?;
        if let ProviderResponse::Status(s) = resp {
            Ok(s)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn get_queue(&self) -> Result<QueueWire, ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::GetQueue).await?;
        if let ProviderResponse::Queue(q) = resp {
            Ok(q)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn enqueue(&self, track: TrackWire) -> Result<(), ProviderProcessError> {
        let resp = self
            .send_request(ProviderRequest::Enqueue { track })
            .await?;
        if resp == ProviderResponse::Ok {
            Ok(())
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn custom_action(
        &self,
        action: &str,
        target: Option<MediaIdWire>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderProcessError> {
        let resp = self
            .send_request(ProviderRequest::Action(ActionRequestV0 {
                provider: self.id.clone(),
                action: action.to_string(),
                target,
                params,
            }))
            .await?;
        if let ProviderResponse::ActionResult(res) = resp {
            Ok(res)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn auth_status(&self) -> Result<AuthStatusWire, ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::GetAuthStatus).await?;
        if let ProviderResponse::AuthStatus(status) = resp {
            Ok(status)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn auth_begin(&self) -> Result<AuthStatusWire, ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::AuthBegin).await?;
        if let ProviderResponse::AuthStatus(status) = resp {
            Ok(status)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }

    pub async fn auth_logout(&self) -> Result<AuthStatusWire, ProviderProcessError> {
        let resp = self.send_request(ProviderRequest::AuthLogout).await?;
        if let ProviderResponse::AuthStatus(status) = resp {
            Ok(status)
        } else {
            Err(ProviderProcessError::UnexpectedResponse(Box::new(resp)))
        }
    }
}

impl Drop for ProviderProcess {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.handle.try_lock()
            && let Some(mut handle) = guard.take()
        {
            let _ = handle.child.start_kill();
        }
    }
}
