//! Backend-neutral web page abstraction.
//!
//! Exposes page navigation, script evaluation, document loading, and event sinks
//! without leaking underlying browser engine or protocol details.

use std::{sync::Arc, time::Duration};

use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, broadcast, oneshot};

use crate::{cdp::BrowserCdpClient, error::WebError, wpe::WpePage};

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

pub(crate) struct ChromiumPageInner {
    target_id: String,
    session_id: String,
    client: BrowserCdpClient,
    event_tx: broadcast::Sender<WebEvent>,
    document_interceptor: Arc<Mutex<Option<DocumentInterceptorState>>>,
    current_url: Arc<Mutex<String>>,
}

impl ChromiumPageInner {
    pub async fn attach(
        client: BrowserCdpClient,
        target_id: String,
        session_id: String,
        initial_url: String,
    ) -> Result<Self, WebError> {
        let (event_tx, _) = broadcast::channel::<WebEvent>(256);
        let interceptor: Arc<Mutex<Option<DocumentInterceptorState>>> = Arc::new(Mutex::new(None));
        let main_frame_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let current_url = Arc::new(Mutex::new(initial_url));

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
        let current_url_bg = current_url.clone();
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
                            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
                        }
                        Some(val) => val.clone(),
                        None => Value::Null,
                    };

                    let _ = page_event_tx.send(WebEvent { name, payload });
                } else if event.method == "Page.frameNavigated" {
                    if event.params.pointer("/frame/parentId").is_none() {
                        if let Some(id) = event.params.pointer("/frame/id").and_then(|v| v.as_str())
                        {
                            *main_frame_id_bg.lock().await = Some(id.to_string());
                        }
                        if let Some(url) =
                            event.params.pointer("/frame/url").and_then(|v| v.as_str())
                        {
                            *current_url_bg.lock().await = url.to_string();
                        }
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
                        && (request_url.contains(&active.url_pattern)
                            || active.url_pattern.contains(&request_url))
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
            current_url,
        })
    }

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
        *self.current_url.lock().await = url.to_string();
        Ok(())
    }

    pub async fn load_document(&self, url: &str, html: &str) -> Result<(), WebError> {
        let body_b64 = BASE64_STANDARD.encode(html.as_bytes());
        let (tx, rx) = oneshot::channel();

        let pattern = if let Ok(parsed) = url::Url::parse(url) {
            parsed.host_str().unwrap_or(url).to_string()
        } else {
            url.to_string()
        };

        {
            let mut guard = self.document_interceptor.lock().await;
            *guard = Some(DocumentInterceptorState {
                url_pattern: pattern,
                status: 200,
                content_type: "text/html; charset=utf-8".to_string(),
                body_base64: body_b64,
                fulfilled_tx: Some(tx),
            });
        }

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
            let _ = self.cancel_document_interception().await;
            return Err(e);
        }

        if let Err(e) = self.navigate(url).await {
            let _ = self.cancel_document_interception().await;
            return Err(e);
        }

        let wait_res = tokio::time::timeout(Duration::from_secs(10), rx).await;
        match wait_res {
            Ok(Ok(())) => {
                *self.current_url.lock().await = url.to_string();
                Ok(())
            }
            Ok(Err(_)) => {
                let _ = self.cancel_document_interception().await;
                Err(WebError::Internal(
                    "Document fulfillment channel dropped".into(),
                ))
            }
            Err(_) => {
                let _ = self.cancel_document_interception().await;
                Err(WebError::Timeout(
                    "Timed out waiting for document fulfillment".into(),
                ))
            }
        }
    }

    async fn cancel_document_interception(&self) -> Result<(), WebError> {
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

    pub fn subscribe_events(&self) -> broadcast::Receiver<WebEvent> {
        self.event_tx.subscribe()
    }

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

enum PageInner {
    Chromium(ChromiumPageInner),
    Wpe(Arc<WpePage>),
}

/// A provider-facing handle to an active web page.
pub struct WebPage {
    inner: PageInner,
}

impl WebPage {
    pub(crate) fn from_chromium(inner: ChromiumPageInner) -> Self {
        Self {
            inner: PageInner::Chromium(inner),
        }
    }

    pub(crate) fn from_wpe(wpe: Arc<WpePage>) -> Self {
        Self {
            inner: PageInner::Wpe(wpe),
        }
    }

    /// Navigate the page to a URL.
    pub async fn navigate(&self, url: &str) -> Result<(), WebError> {
        match &self.inner {
            PageInner::Chromium(c) => c.navigate(url).await,
            PageInner::Wpe(w) => w.navigate(url).await,
        }
    }

    /// Load an explicit HTML document rooted at `url` origin.
    ///
    /// For Chromium, this is achieved via one-shot document interception.
    /// For WPE, this is achieved via native alternate HTML loading.
    pub async fn load_document(&self, url: &str, html: &str) -> Result<(), WebError> {
        match &self.inner {
            PageInner::Chromium(c) => c.load_document(url, html).await,
            PageInner::Wpe(w) => w.load_document(url, html).await,
        }
    }

    /// Reload the page.
    pub async fn reload(&self) -> Result<(), WebError> {
        match &self.inner {
            PageInner::Chromium(c) => c.reload().await,
            PageInner::Wpe(w) => w.reload().await,
        }
    }

    /// Evaluate a JavaScript expression in the page context and return its result as JSON.
    pub async fn evaluate(&self, expression: &str) -> Result<Value, WebError> {
        match &self.inner {
            PageInner::Chromium(c) => c.evaluate(expression).await,
            PageInner::Wpe(w) => w.evaluate(expression).await,
        }
    }

    /// Call a JavaScript function on the `window` object with structured arguments.
    pub async fn call_function(
        &self,
        function_declaration: &str,
        arguments: &[Value],
    ) -> Result<Value, WebError> {
        match &self.inner {
            PageInner::Chromium(c) => c.call_function(function_declaration, arguments).await,
            PageInner::Wpe(w) => w.call_function(function_declaration, arguments).await,
        }
    }

    /// Call a JavaScript function on the `window` object with structured arguments and a custom timeout.
    pub async fn call_function_with_timeout(
        &self,
        function_declaration: &str,
        arguments: &[Value],
        timeout: Duration,
    ) -> Result<Value, WebError> {
        match &self.inner {
            PageInner::Chromium(c) => {
                c.call_function_with_timeout(function_declaration, arguments, timeout)
                    .await
            }
            PageInner::Wpe(w) => {
                w.call_function_with_timeout(function_declaration, arguments, timeout)
                    .await
            }
        }
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
        match &self.inner {
            PageInner::Chromium(c) => c.register_event_sink(name).await,
            PageInner::Wpe(w) => w.register_event_sink(name).await,
        }
    }

    /// Subscribe to web events triggered by registered event sinks.
    pub fn subscribe_events(&self) -> broadcast::Receiver<WebEvent> {
        match &self.inner {
            PageInner::Chromium(c) => c.subscribe_events(),
            PageInner::Wpe(w) => w.subscribe_events(),
        }
    }

    /// Diagnostic health query for this page target.
    pub async fn check_health(&self) -> Result<PageHealth, WebError> {
        match &self.inner {
            PageInner::Chromium(c) => c.check_health().await,
            PageInner::Wpe(w) => w.check_health().await,
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
