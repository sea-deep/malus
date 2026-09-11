//! Backend-neutral web page abstraction.
//!
//! Exposes page navigation, script evaluation, and event sinks without
//! leaking underlying protocol details.

use std::{sync::Arc, time::Duration};

use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, broadcast, oneshot};

use crate::{cdp::BrowserCdpClient, error::WebError};

struct DocumentInterceptorState {
    url_pattern: String,
    status: u16,
    content_type: String,
    body_base64: String,
    fulfilled_tx: Option<oneshot::Sender<()>>,
}

/// A push event received from a registered web event sink.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebEvent {
    /// The name of the event sink that triggered this notification.
    pub name: String,
    /// The payload passed by the web script.
    pub payload: Value,
}

/// Diagnostic health information for an active web page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageHealth {
    pub target_id: String,
    pub session_id: String,
    pub connected: bool,
    pub url: String,
}

/// A provider-facing handle to an active web page.
pub struct WebPage {
    target_id: String,
    session_id: String,
    client: BrowserCdpClient,
    event_tx: broadcast::Sender<WebEvent>,
    document_interceptor: Arc<Mutex<Option<DocumentInterceptorState>>>,
}

impl WebPage {
    /// Initialize and attach to a page target on the browser client.
    pub async fn attach(
        client: BrowserCdpClient,
        target_id: String,
        session_id: String,
    ) -> Result<Self, WebError> {
        let (event_tx, _) = broadcast::channel::<WebEvent>(256);
        let interceptor: Arc<Mutex<Option<DocumentInterceptorState>>> = Arc::new(Mutex::new(None));
        let main_frame_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // Enable Page and Runtime domains for this session
        client
            .send_command(
                Some(&session_id),
                "Page.enable",
                Value::Null,
                Duration::from_secs(5),
            )
            .await?;

        client
            .send_command(
                Some(&session_id),
                "Runtime.enable",
                Value::Null,
                Duration::from_secs(5),
            )
            .await?;

        if let Ok(tree_res) = client
            .send_command(
                Some(&session_id),
                "Page.getFrameTree",
                Value::Null,
                Duration::from_secs(5),
            )
            .await
            && let Some(id) = tree_res
                .pointer("/frameTree/frame/id")
                .and_then(|v| v.as_str())
        {
            *main_frame_id.lock().await = Some(id.to_string());
        }

        // Spawn a background event demultiplexer for this session
        let mut raw_rx = client.subscribe_events();
        let session_filter = session_id.clone();
        let page_event_tx = event_tx.clone();
        let interceptor_bg = interceptor.clone();
        let main_frame_id_bg = main_frame_id.clone();
        let client_bg = client.clone();

        tokio::spawn(async move {
            while let Ok(event) = raw_rx.recv().await {
                if event.session_id.as_deref() != Some(&session_filter) {
                    continue;
                }

                if event.method == "Runtime.bindingCalled" {
                    let name = event
                        .params
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    let raw_payload = event.params.get("payload");
                    let payload = match raw_payload {
                        Some(Value::String(s)) => {
                            // Try parsing as JSON; if not valid JSON, preserve raw string
                            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
                        }
                        Some(val) => val.clone(),
                        None => Value::Null,
                    };

                    let _ = page_event_tx.send(WebEvent { name, payload });
                } else if event.method == "Page.frameNavigated" {
                    if event.params.pointer("/frame/parentId").is_none()
                        && let Some(id) = event.params.pointer("/frame/id").and_then(|v| v.as_str())
                    {
                        *main_frame_id_bg.lock().await = Some(id.to_string());
                    }
                } else if event.method == "Fetch.requestPaused" {
                    let request_id = event
                        .params
                        .get("requestId")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let resource_type = event
                        .params
                        .get("resourceType")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let frame_id = event
                        .params
                        .get("frameId")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let request_url = event
                        .params
                        .pointer("/request/url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    let mut guard = interceptor_bg.lock().await;
                    let cur_main_frame = main_frame_id_bg.lock().await.clone();
                    let is_main_frame = cur_main_frame.as_deref().is_none_or(|mf| mf == frame_id);

                    if resource_type == "Document"
                        && is_main_frame
                        && let Some(active) = guard.as_mut()
                        && request_url.contains(&active.url_pattern)
                    {
                        let fulfill_params = serde_json::json!({
                            "requestId": request_id,
                            "responseCode": active.status,
                            "responseHeaders": [
                                { "name": "content-type", "value": active.content_type }
                            ],
                            "body": active.body_base64
                        });

                        let _ = client_bg
                            .send_command(
                                Some(&session_filter),
                                "Fetch.fulfillRequest",
                                fulfill_params,
                                Duration::from_secs(5),
                            )
                            .await;

                        if let Some(tx) = active.fulfilled_tx.take() {
                            let _ = tx.send(());
                        }
                        *guard = None;

                        // Immediately disable Fetch domain
                        let _ = client_bg
                            .send_command(
                                Some(&session_filter),
                                "Fetch.disable",
                                serde_json::json!({}),
                                Duration::from_secs(5),
                            )
                            .await;
                    } else {
                        let continue_params = serde_json::json!({ "requestId": request_id });
                        let _ = client_bg
                            .send_command(
                                Some(&session_filter),
                                "Fetch.continueRequest",
                                continue_params,
                                Duration::from_secs(5),
                            )
                            .await;
                    }
                }
            }
        });

        Ok(Self {
            target_id,
            session_id,
            client,
            event_tx,
            document_interceptor: interceptor,
        })
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Navigate the page to a URL.
    pub async fn navigate(&self, url: &str) -> Result<(), WebError> {
        let params = serde_json::json!({ "url": url });
        self.client
            .send_command(
                Some(&self.session_id),
                "Page.navigate",
                params,
                Duration::from_secs(15),
            )
            .await?;
        Ok(())
    }

    /// Reload the page.
    pub async fn reload(&self) -> Result<(), WebError> {
        self.client
            .send_command(
                Some(&self.session_id),
                "Page.reload",
                serde_json::json!({}),
                Duration::from_secs(15),
            )
            .await?;
        Ok(())
    }

    /// Intercept the next top-level document navigation matching `url_pattern` and fulfill it with `body`.
    ///
    /// This is strictly one-shot for the main frame. Immediately after fulfillment, or if cancelled,
    /// the interceptor state is cleared and `Fetch.disable` is invoked.
    pub async fn intercept_next_main_document(
        &self,
        url_pattern: &str,
        content_type: &str,
        body: &str,
    ) -> Result<oneshot::Receiver<()>, WebError> {
        let body_b64 = BASE64_STANDARD.encode(body.as_bytes());
        let (tx, rx) = oneshot::channel();

        let mut guard = self.document_interceptor.lock().await;
        *guard = Some(DocumentInterceptorState {
            url_pattern: url_pattern.to_string(),
            status: 200,
            content_type: content_type.to_string(),
            body_base64: body_b64,
            fulfilled_tx: Some(tx),
        });

        // Enable Fetch domain for Document resources
        let params = serde_json::json!({
            "patterns": [
                {
                    "urlPattern": "*",
                    "resourceType": "Document",
                    "requestStage": "Request"
                }
            ]
        });

        if let Err(e) = self
            .client
            .send_command(
                Some(&self.session_id),
                "Fetch.enable",
                params,
                Duration::from_secs(5),
            )
            .await
        {
            *guard = None;
            return Err(e);
        }

        Ok(rx)
    }

    /// Cancel any active document interceptor and unconditionally disable the CDP Fetch domain.
    pub async fn cancel_document_interception(&self) -> Result<(), WebError> {
        {
            let mut guard = self.document_interceptor.lock().await;
            *guard = None;
        }
        let _ = self
            .client
            .send_command(
                Some(&self.session_id),
                "Fetch.disable",
                serde_json::json!({}),
                Duration::from_secs(5),
            )
            .await;
        Ok(())
    }

    /// Evaluate a JavaScript expression in the page context and return its result as JSON.
    pub async fn evaluate(&self, expression: &str) -> Result<Value, WebError> {
        let params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        });

        let res = self
            .client
            .send_command(
                Some(&self.session_id),
                "Runtime.evaluate",
                params,
                Duration::from_secs(15),
            )
            .await?;

        if let Some(exception) = res.get("exceptionDetails") {
            return Err(extract_js_exception(exception));
        }

        Ok(res
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Call a JavaScript function on the `window` object with structured arguments.
    /// Call a JavaScript function on the `window` object with structured arguments and a custom timeout.
    pub async fn call_function_with_timeout(
        &self,
        function_declaration: &str,
        arguments: &[Value],
        timeout: Duration,
    ) -> Result<Value, WebError> {
        // Evaluate window to get an objectId for the call context
        let window_res = self
            .client
            .send_command(
                Some(&self.session_id),
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": "window",
                    "returnByValue": false,
                }),
                Duration::from_secs(15),
            )
            .await?;

        let object_id = window_res
            .get("result")
            .and_then(|r| r.get("objectId"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                WebError::Evaluation("Failed to resolve window execution context".to_string())
            })?;

        let call_args: Vec<Value> = arguments
            .iter()
            .map(|arg| serde_json::json!({ "value": arg }))
            .collect();

        let params = serde_json::json!({
            "objectId": object_id,
            "functionDeclaration": function_declaration,
            "arguments": call_args,
            "returnByValue": true,
            "awaitPromise": true,
        });

        let result = self
            .client
            .send_command(
                Some(&self.session_id),
                "Runtime.callFunctionOn",
                params,
                timeout,
            )
            .await;

        // Release remote object handle unconditionally
        let _ = self
            .client
            .send_command(
                Some(&self.session_id),
                "Runtime.releaseObject",
                serde_json::json!({ "objectId": object_id }),
                Duration::from_millis(500),
            )
            .await;

        let res = result?;
        if let Some(exception) = res.get("exceptionDetails") {
            return Err(extract_js_exception(exception));
        }

        Ok(res
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Call a JavaScript function on the `window` object with structured arguments.
    pub async fn call_function(
        &self,
        function_declaration: &str,
        arguments: &[Value],
    ) -> Result<Value, WebError> {
        self.call_function_with_timeout(function_declaration, arguments, Duration::from_secs(30))
            .await
    }

    /// Repeatedly evaluate a JavaScript expression until it produces a truthy value or times out.
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

    /// Register a named event sink callable from web scripts as `window.<name>(payload)`.
    pub async fn register_event_sink(&self, name: &str) -> Result<(), WebError> {
        let params = serde_json::json!({ "name": name });
        self.client
            .send_command(
                Some(&self.session_id),
                "Runtime.addBinding",
                params,
                Duration::from_secs(5),
            )
            .await?;
        Ok(())
    }

    /// Subscribe to web events triggered by registered event sinks.
    pub fn subscribe_events(&self) -> broadcast::Receiver<WebEvent> {
        self.event_tx.subscribe()
    }

    /// Access the underlying CDP client.
    pub fn client(&self) -> &BrowserCdpClient {
        &self.client
    }

    /// Minimize the browser window displaying this page via CDP.
    pub async fn minimize_window(&self) -> Result<(), WebError> {
        let (window_id, _) = self.client.get_window_for_target(&self.target_id).await?;
        self.client
            .set_window_bounds(
                window_id,
                serde_json::json!({
                    "windowState": "minimized"
                }),
            )
            .await?;
        Ok(())
    }

    /// Diagnostic health query for this page target.
    pub async fn check_health(&self) -> Result<PageHealth, WebError> {
        let targets = self.client.get_targets().await?;
        let current_target = targets.into_iter().find(|t| t.target_id == self.target_id);

        match current_target {
            Some(info) => Ok(PageHealth {
                target_id: self.target_id.clone(),
                session_id: self.session_id.clone(),
                connected: self.client.is_connected(),
                url: info.url,
            }),
            None => Ok(PageHealth {
                target_id: self.target_id.clone(),
                session_id: self.session_id.clone(),
                connected: false,
                url: String::new(),
            }),
        }
    }
}

fn extract_js_exception(details: &Value) -> WebError {
    let msg = details["exception"]["description"]
        .as_str()
        .or_else(|| details["text"].as_str())
        .unwrap_or("JavaScript exception");
    let clean = msg.lines().next().unwrap_or(msg);
    WebError::Evaluation(clean.chars().take(300).collect())
}
