//! Chrome DevTools Protocol (CDP) client and browser session manager.
//!
//! Connects directly to the browser-level WebSocket endpoint discovered from
//! `DevToolsActivePort`, discovers and manages targets via `Target.*`, and attaches
//! to page sessions using flattened sessions (`Target.attachToTarget` with `flatten: true`).
//!
//! Note: CDP messages use `{ id, method, params }` request framing and
//! `{ id, result, error }` response framing (distinct from JSON-RPC 2.0).

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::timeout,
};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::error::WebError;

#[derive(Debug, Serialize)]
struct CdpCommandWire {
    id: u64,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct CdpResponseWire {
    id: Option<u64>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct RawCdpEvent {
    pub session_id: Option<String>,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetInfo {
    #[serde(rename = "targetId")]
    pub target_id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub title: String,
    pub url: String,
    pub attached: bool,
}

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, WebError>>>>>;

struct PendingGuard {
    id: u64,
    pending: PendingMap,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.pending.lock() {
            map.remove(&self.id);
        }
    }
}

/// Browser-level CDP client connected to the main browser WebSocket endpoint.
#[derive(Clone)]
pub struct BrowserCdpClient {
    next_id: Arc<AtomicU64>,
    tx_cmd: mpsc::Sender<CdpCommandWire>,
    pending: PendingMap,
    connected: Arc<AtomicBool>,
    event_tx: broadcast::Sender<RawCdpEvent>,
}

impl BrowserCdpClient {
    /// Establish a WebSocket connection to the browser-level endpoint.
    pub async fn connect(browser_ws_url: &str) -> Result<Self, WebError> {
        let (ws_stream, _) = timeout(Duration::from_secs(6), connect_async(browser_ws_url))
            .await
            .map_err(|_| {
                WebError::Connection(format!(
                    "Timed out connecting to browser WebSocket at {}",
                    browser_ws_url
                ))
            })?
            .map_err(|e| {
                WebError::Connection(format!(
                    "Failed to connect to browser WebSocket at {}: {}",
                    browser_ws_url, e
                ))
            })?;

        let (mut write, mut read) = ws_stream.split();
        let (tx_cmd, mut rx_cmd) = mpsc::channel::<CdpCommandWire>(128);
        let (event_tx, _) = broadcast::channel::<RawCdpEvent>(256);

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let connected = Arc::new(AtomicBool::new(true));

        // Background write loop
        let pending_write = pending.clone();
        let connected_write = connected.clone();
        tokio::spawn(async move {
            while let Some(cmd) = rx_cmd.recv().await {
                let text = match serde_json::to_string(&cmd) {
                    Ok(t) => t,
                    Err(e) => {
                        if let Ok(mut map) = pending_write.lock()
                            && let Some(sender) = map.remove(&cmd.id)
                        {
                            let _ = sender.send(Err(WebError::Protocol(format!(
                                "Serialization failure: {}",
                                e
                            ))));
                        }
                        continue;
                    }
                };

                if timeout(
                    Duration::from_secs(5),
                    write.send(Message::Text(text.into())),
                )
                .await
                .map_or(true, |r| r.is_err())
                {
                    connected_write.store(false, Ordering::Release);
                    fail_all_pending(
                        &pending_write,
                        "Failed to write message to browser WebSocket",
                    );
                    break;
                }
            }
        });

        // Background read loop
        let pending_read = pending.clone();
        let connected_read = connected.clone();
        let event_tx_read = event_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_res) = read.next().await {
                match msg_res {
                    Ok(Message::Text(text)) => {
                        if let Ok(response) = serde_json::from_str::<CdpResponseWire>(&text) {
                            if let Some(id) = response.id {
                                // Match command response
                                let sender =
                                    pending_read.lock().ok().and_then(|mut map| map.remove(&id));
                                if let Some(sender) = sender {
                                    let result = match response.error {
                                        Some(err) => {
                                            let msg = err
                                                .get("message")
                                                .and_then(|m| m.as_str())
                                                .unwrap_or("CDP command failed");
                                            Err(WebError::Protocol(msg.to_string()))
                                        }
                                        None => Ok(response.result.unwrap_or(Value::Null)),
                                    };
                                    let _ = sender.send(result);
                                }
                            } else if let Some(method) = response.method {
                                // Push event
                                let _ = event_tx_read.send(RawCdpEvent {
                                    session_id: response.session_id,
                                    method,
                                    params: response.params.unwrap_or(Value::Null),
                                });
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => {
                        break;
                    }
                    _ => {}
                }
            }

            connected_read.store(false, Ordering::Release);
            fail_all_pending(&pending_read, "Browser WebSocket closed");
        });

        Ok(Self {
            next_id: Arc::new(AtomicU64::new(1)),
            tx_cmd,
            pending,
            connected,
            event_tx,
        })
    }

    /// Whether the WebSocket connection is actively open.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// Subscribe to raw browser-level events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<RawCdpEvent> {
        self.event_tx.subscribe()
    }

    /// Send a command to the browser session (or to a specific page session if `session_id` is given).
    pub async fn send_command(
        &self,
        session_id: Option<&str>,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> Result<Value, WebError> {
        if !self.is_connected() {
            return Err(WebError::Disconnected(
                "Browser WebSocket is disconnected".to_string(),
            ));
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let wire = CdpCommandWire {
            id,
            session_id: session_id.map(|s| s.to_string()),
            method: method.to_string(),
            params,
        };

        let (sender, receiver) = oneshot::channel();
        if let Ok(mut map) = self.pending.lock() {
            map.insert(id, sender);
        } else {
            return Err(WebError::Disconnected("Pending map poisoned".to_string()));
        }

        let _guard = PendingGuard {
            id,
            pending: self.pending.clone(),
        };

        timeout(deadline, async {
            self.tx_cmd
                .send(wire)
                .await
                .map_err(|_| WebError::Disconnected("CDP write loop terminated".to_string()))?;
            receiver
                .await
                .map_err(|_| WebError::Disconnected("CDP response sender dropped".to_string()))?
        })
        .await
        .map_err(|_| {
            WebError::Timeout(format!(
                "CDP command '{}' timed out after {:?}",
                method, deadline
            ))
        })?
    }

    /// Query the browser version for diagnostics (`Browser.getVersion`).
    pub async fn get_browser_version(&self) -> Result<Value, WebError> {
        self.send_command(
            None,
            "Browser.getVersion",
            Value::Null,
            Duration::from_secs(5),
        )
        .await
    }

    /// Discover available browser targets (`Target.getTargets`).
    pub async fn get_targets(&self) -> Result<Vec<TargetInfo>, WebError> {
        let res = self
            .send_command(
                None,
                "Target.getTargets",
                Value::Null,
                Duration::from_secs(5),
            )
            .await?;

        let targets = res
            .get("targetInfos")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));

        serde_json::from_value(targets)
            .map_err(|e| WebError::Protocol(format!("Failed to parse targetInfos: {}", e)))
    }

    /// Create a new target (`Target.createTarget`).
    pub async fn create_target(&self, url: &str) -> Result<String, WebError> {
        let params = serde_json::json!({ "url": url });
        let res = self
            .send_command(None, "Target.createTarget", params, Duration::from_secs(10))
            .await?;

        res.get("targetId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| WebError::Target("Target.createTarget missing targetId".to_string()))
    }

    /// Attach to a target with flattening (`Target.attachToTarget` with `flatten: true`).
    ///
    /// Returns the assigned `sessionId`.
    pub async fn attach_to_target(&self, target_id: &str) -> Result<String, WebError> {
        let params = serde_json::json!({
            "targetId": target_id,
            "flatten": true,
        });

        let res = self
            .send_command(
                None,
                "Target.attachToTarget",
                params,
                Duration::from_secs(10),
            )
            .await?;

        res.get("sessionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| WebError::Target("Target.attachToTarget missing sessionId".to_string()))
    }

    /// Close a target (`Target.closeTarget`).
    pub async fn close_target(&self, target_id: &str) -> Result<bool, WebError> {
        let params = serde_json::json!({ "targetId": target_id });
        let res = self
            .send_command(None, "Target.closeTarget", params, Duration::from_secs(5))
            .await?;

        Ok(res.get("success").and_then(|v| v.as_bool()).unwrap_or(true))
    }

    /// Get the browser window containing a target (`Browser.getWindowForTarget`).
    pub async fn get_window_for_target(&self, target_id: &str) -> Result<(u32, Value), WebError> {
        let params = serde_json::json!({ "targetId": target_id });
        let res = self
            .send_command(
                None,
                "Browser.getWindowForTarget",
                params,
                Duration::from_secs(5),
            )
            .await?;

        let window_id = res
            .get("windowId")
            .and_then(|v| v.as_u64())
            .map(|w| w as u32)
            .ok_or_else(|| {
                WebError::Protocol("Browser.getWindowForTarget missing windowId".to_string())
            })?;

        let bounds = res.get("bounds").cloned().unwrap_or(Value::Null);
        Ok((window_id, bounds))
    }

    /// Set browser window bounds/state (`Browser.setWindowBounds`).
    pub async fn set_window_bounds(&self, window_id: u32, bounds: Value) -> Result<(), WebError> {
        let params = serde_json::json!({
            "windowId": window_id,
            "bounds": bounds,
        });
        self.send_command(
            None,
            "Browser.setWindowBounds",
            params,
            Duration::from_secs(5),
        )
        .await?;
        Ok(())
    }
}

fn fail_all_pending(pending: &PendingMap, message: &str) {
    if let Ok(mut map) = pending.lock() {
        for (_, sender) in map.drain() {
            let _ = sender.send(Err(WebError::Disconnected(message.to_string())));
        }
    }
}
