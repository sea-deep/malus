//! Backend-neutral web page abstraction.
//!
//! Exposes page navigation, script evaluation, and event sinks without
//! leaking underlying protocol details.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::{cdp::BrowserCdpClient, error::WebError};

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
}

impl WebPage {
    /// Initialize and attach to a page target on the browser client.
    pub async fn attach(
        client: BrowserCdpClient,
        target_id: String,
        session_id: String,
    ) -> Result<Self, WebError> {
        let (event_tx, _) = broadcast::channel::<WebEvent>(256);

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

        // Spawn a background event demultiplexer for this session
        let mut raw_rx = client.subscribe_events();
        let session_filter = session_id.clone();
        let page_event_tx = event_tx.clone();

        tokio::spawn(async move {
            while let Ok(event) = raw_rx.recv().await {
                if event.session_id.as_deref() == Some(&session_filter)
                    && event.method == "Runtime.bindingCalled"
                {
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
                }
            }
        });

        Ok(Self {
            target_id,
            session_id,
            client,
            event_tx,
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
    pub async fn call_function(
        &self,
        function_declaration: &str,
        arguments: &[Value],
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
                Duration::from_secs(5),
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
                Duration::from_secs(15),
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
