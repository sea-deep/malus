//! Chrome DevTools Protocol (CDP) WebSocket client for Malus.
//!
//! Handles JSON-RPC 2.0 transport, bidirectional command dispatching,
//! push-event routing via `Runtime.bindingCalled`, and keepalive heartbeats.

use std::{
    collections::HashMap,
    io::{self, ErrorKind},
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
    time::sleep,
};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Clone, Deserialize)]
pub struct TargetInfo {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub websocket_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct CdpRequest {
    id: u64,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct CdpResponse {
    id: Option<u64>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

pub struct CdpClient {
    next_id: AtomicU64,
    tx_cmd: mpsc::Sender<CdpRequest>,
    pending: Pending,
    connected: Arc<AtomicBool>,
    pub event_rx: broadcast::Receiver<CdpEvent>,
}

#[derive(Debug, Clone)]
pub struct CdpEvent {
    pub method: String,
    pub params: Value,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<io::Result<Value>>>>>;
struct PendingRequest {
    id: u64,
    pending: Pending,
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.id);
        }
    }
}
fn fail_pending(pending: &Pending, message: &str) {
    if let Ok(mut pending) = pending.lock() {
        for (_, sender) in pending.drain() {
            let _ = sender.send(Err(io::Error::new(
                ErrorKind::BrokenPipe,
                message.to_string(),
            )));
        }
    }
}

impl CdpClient {
    /// Connect to the active target page on the specified local port.
    pub async fn connect_to_page(port: u16, target_url_contains: &str) -> io::Result<Self> {
        let ws_url = resolve_page_websocket_url(port, target_url_contains).await?;
        Self::connect_ws(&ws_url).await
    }

    /// Connect directly to a WebSocket URL.
    pub async fn connect_ws(ws_url: &str) -> io::Result<Self> {
        let (ws_stream, _) = tokio::time::timeout(Duration::from_secs(5), connect_async(ws_url))
            .await
            .map_err(|_| io::Error::new(ErrorKind::TimedOut, "CDP connection timed out"))?
            .map_err(|e| {
                io::Error::new(ErrorKind::ConnectionRefused, format!("CDP WS error: {}", e))
            })?;

        let (mut write, mut read) = ws_stream.split();
        let (tx_cmd, mut rx_cmd) = mpsc::channel::<CdpRequest>(64);
        let (tx_event, event_rx) = broadcast::channel::<CdpEvent>(128);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let connected = Arc::new(AtomicBool::new(true));
        let pending_write = pending.clone();
        let connected_write = connected.clone();
        tokio::spawn(async move {
            while let Some(req) = rx_cmd.recv().await {
                if !pending_write.lock().unwrap().contains_key(&req.id) {
                    continue;
                }
                let message = match serde_json::to_string(&req) {
                    Ok(m) => m,
                    Err(e) => {
                        if let Some(sender) = pending_write.lock().unwrap().remove(&req.id) {
                            let _ = sender.send(Err(io::Error::new(ErrorKind::InvalidData, e)));
                        }
                        continue;
                    }
                };
                if tokio::time::timeout(
                    Duration::from_secs(5),
                    write.send(Message::Text(message.into())),
                )
                .await
                .map_or(true, |r| r.is_err())
                {
                    connected_write.store(false, Ordering::Release);
                    fail_pending(&pending_write, "Could not write to the Apple Music browser");
                    break;
                }
            }
        });
        let pending_read = pending.clone();
        let connected_read = connected.clone();
        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        if let Ok(response) = serde_json::from_str::<CdpResponse>(&text) {
                            if let Some(id) = response.id {
                                if let Some(sender) = pending_read.lock().unwrap().remove(&id) {
                                    let result = match response.error {
                                        Some(error) => Err(io::Error::other(
                                            error["message"]
                                                .as_str()
                                                .unwrap_or("CDP request failed")
                                                .to_string(),
                                        )),
                                        None => Ok(response.result.unwrap_or(Value::Null)),
                                    };
                                    let _ = sender.send(result);
                                }
                            } else if let Some(method) = response.method {
                                let _ = tx_event.send(CdpEvent {
                                    method,
                                    params: response.params.unwrap_or(Value::Null),
                                });
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            connected_read.store(false, Ordering::Release);
            fail_pending(&pending_read, "Apple Music disconnected");
        });
        Ok(Self {
            next_id: AtomicU64::new(1),
            tx_cmd,
            pending,
            connected,
            event_rx,
        })
    }
    /// Every command, including admission to the writer queue, has a deadline.
    pub async fn send_command(
        &self,
        method: impl Into<String>,
        params: Value,
    ) -> io::Result<Value> {
        self.send_command_timeout(method, params, Duration::from_secs(15))
            .await
    }
    pub async fn send_command_timeout(
        &self,
        method: impl Into<String>,
        params: Value,
        deadline: Duration,
    ) -> io::Result<Value> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(io::Error::new(
                ErrorKind::NotConnected,
                "Apple Music disconnected",
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = CdpRequest {
            id,
            method: method.into(),
            params,
        };
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, sender);
        // Synchronous cleanup also runs if the caller cancels a search task.
        let _pending = PendingRequest {
            id,
            pending: self.pending.clone(),
        };
        tokio::time::timeout(deadline, async {
            self.tx_cmd
                .send(req)
                .await
                .map_err(|_| io::Error::new(ErrorKind::BrokenPipe, "CDP writer stopped"))?;
            receiver
                .await
                .map_err(|_| io::Error::new(ErrorKind::UnexpectedEof, "CDP response dropped"))?
        })
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, "Apple Music request timed out"))?
    }

    /// Evaluate a JavaScript expression in the page context.
    pub async fn evaluate_js(&self, expression: &str) -> io::Result<Value> {
        let params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        });

        let res = self.send_command("Runtime.evaluate", params).await?;
        if let Some(exception) = res.get("exceptionDetails") {
            return Err(js_exception(exception));
        }

        Ok(res
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Call a function with structured JSON arguments (OWASP A03: Injection prevention).
    pub async fn call_function(
        &self,
        function_declaration: &str,
        arguments: &[Value],
    ) -> io::Result<Value> {
        // Obtain the window objectId so callFunctionOn has a valid target execution context
        let window_res = self
            .send_command(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": "window",
                    "returnByValue": false
                }),
            )
            .await?;

        let object_id = window_res
            .get("result")
            .and_then(|r| r.get("objectId"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                io::Error::new(ErrorKind::NotFound, "Failed to obtain window objectId")
            })?;

        let cdp_args: Vec<Value> = arguments
            .iter()
            .map(|arg| serde_json::json!({ "value": arg }))
            .collect();
        let params = serde_json::json!({
            "objectId": object_id,
            "functionDeclaration": function_declaration,
            "arguments": cdp_args,
            "returnByValue": true,
            "awaitPromise": true,
        });

        let result = self.send_command("Runtime.callFunctionOn", params).await;
        // Release the temporary remote handle, including when the JS promise rejects.
        let _ = self
            .send_command_timeout(
                "Runtime.releaseObject",
                serde_json::json!({"objectId":object_id}),
                Duration::from_millis(500),
            )
            .await;
        let res = result?;
        if let Some(exception) = res.get("exceptionDetails") {
            return Err(js_exception(exception));
        }

        Ok(res
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Inject a native binding callable from JavaScript as `window.<name>(payload)`.
    pub async fn add_binding(&self, name: &str) -> io::Result<()> {
        let params = serde_json::json!({ "name": name });
        self.send_command("Runtime.addBinding", params).await?;
        Ok(())
    }

    /// Send keepalive ping to ensure connection remains alive.
    pub async fn ping(&self) -> io::Result<()> {
        self.send_command("Browser.getVersion", Value::Null).await?;
        Ok(())
    }
}

fn js_exception(value: &Value) -> io::Error {
    let message = value["exception"]["description"]
        .as_str()
        .or_else(|| value["text"].as_str())
        .unwrap_or("Apple Music request failed");
    io::Error::other(
        message
            .lines()
            .next()
            .unwrap_or(message)
            .chars()
            .take(240)
            .collect::<String>(),
    )
}

/// Query `http://127.0.0.1:<port>/json/list` to find the target page WebSocket URL.
async fn resolve_page_websocket_url(port: u16, target_url_contains: &str) -> io::Result<String> {
    let list_endpoint = format!("http://127.0.0.1:{}/json/list", port);
    let start = std::time::Instant::now();
    let mut last_err = String::new();

    while start.elapsed() < Duration::from_secs(12) {
        match fetch_http_text(&list_endpoint).await {
            Ok(resp_text) => {
                match serde_json::from_str::<Vec<Value>>(&resp_text) {
                    Ok(targets) => {
                        // 1. Find page target matching target_url_contains
                        for t in &targets {
                            let t_type = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
                            let ws = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str());

                            if t_type == "page"
                                && url.contains(target_url_contains)
                                && let Some(ws_url) = ws
                            {
                                return Ok(ws_url.to_string());
                            }
                        }

                        last_err = format!(
                            "Found {} targets, but none was a page with webSocketDebuggerUrl",
                            targets.len()
                        );
                    }
                    Err(e) => {
                        last_err = format!("JSON parse error: {} on response: {}", e, resp_text);
                    }
                }
            }
            Err(e) => {
                last_err = format!("HTTP fetch error: {}", e);
            }
        }
        sleep(Duration::from_millis(250)).await;
    }

    Err(io::Error::new(
        ErrorKind::NotFound,
        format!(
            "Timed out finding target page at port {} (last error: {})",
            port, last_err
        ),
    ))
}

/// Minimal non-blocking HTTP GET using standard library / tokio with Content-Length support.
async fn fetch_http_text(url_str: &str) -> io::Result<String> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let parsed =
            url::Url::parse(url_str).map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;
        let host = parsed.host_str().unwrap_or("127.0.0.1");
        let port = parsed.port().unwrap_or(80);
        let path = parsed.path();

        let mut stream = tokio::net::TcpStream::connect((host, port)).await?;
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
            path, host, port
        );
        tokio::io::AsyncWriteExt::write_all(&mut stream, req.as_bytes()).await?;

        let mut buf = Vec::new();
        let mut temp = [0u8; 1024];

        let mut header_end = None;
        let mut content_length = None;

        loop {
            let n = tokio::io::AsyncReadExt::read(&mut stream, &mut temp).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&temp[..n]);

            if header_end.is_none()
                && let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n")
            {
                header_end = Some(pos + 4);
                let header_str = String::from_utf8_lossy(&buf[..pos]);
                for line in header_str.lines() {
                    let lower = line.to_ascii_lowercase();
                    if let Some(val) = lower.strip_prefix("content-length:")
                        && let Ok(len) = val.trim().parse::<usize>()
                    {
                        content_length = Some(len);
                    }
                }
            }

            if let (Some(start), Some(cl)) = (header_end, content_length)
                && buf.len() >= start + cl
            {
                break;
            }
        }

        if let Some(start) = header_end {
            if let Some(cl) = content_length {
                let end = (start + cl).min(buf.len());
                return Ok(String::from_utf8_lossy(&buf[start..end]).to_string());
            }
            return Ok(String::from_utf8_lossy(&buf[start..]).to_string());
        }

        Ok(String::from_utf8_lossy(&buf).to_string())
    })
    .await
    .map_err(|_| io::Error::new(ErrorKind::TimedOut, "HTTP request timed out"))?
}
