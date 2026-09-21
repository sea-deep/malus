//! WPE WebKit page facade.
//!
//! Exposes page navigation, script evaluation, document loading, and event sinks.

use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::{error::WebError, wpe::WpePage};

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
    inner: Arc<WpePage>,
}

impl WebPage {
    pub(crate) fn from_wpe(wpe: Arc<WpePage>) -> Self {
        Self { inner: wpe }
    }

    /// Navigate the page to a URL.
    pub async fn navigate(&self, url: &str) -> Result<(), WebError> {
        self.inner.navigate(url).await
    }

    /// Load an explicit HTML document rooted at `url` origin via native alternate HTML loading.
    pub async fn load_document(&self, url: &str, html: &str) -> Result<(), WebError> {
        self.inner.load_document(url, html).await
    }

    /// Reload the page.
    pub async fn reload(&self) -> Result<(), WebError> {
        self.inner.reload().await
    }

    /// Evaluate a JavaScript expression in the page context and return its result as JSON.
    pub async fn evaluate(&self, expression: &str) -> Result<Value, WebError> {
        self.inner.evaluate(expression).await
    }

    /// Call a JavaScript function on the `window` object with structured arguments.
    pub async fn call_function(
        &self,
        function_declaration: &str,
        arguments: &[Value],
    ) -> Result<Value, WebError> {
        self.inner
            .call_function(function_declaration, arguments)
            .await
    }

    /// Call a JavaScript function on the `window` object with structured arguments and a custom timeout.
    pub async fn call_function_with_timeout(
        &self,
        function_declaration: &str,
        arguments: &[Value],
        timeout: Duration,
    ) -> Result<Value, WebError> {
        self.inner
            .call_function_with_timeout(function_declaration, arguments, timeout)
            .await
    }

    /// Repeatedly evaluate a JavaScript expression until it produces a truthy value or times out.
    pub async fn wait_for_expression(
        &self,
        expression: &str,
        timeout_duration: Duration,
    ) -> Result<Value, WebError> {
        self.inner
            .wait_for_expression(expression, timeout_duration)
            .await
    }

    /// Register a named event sink callable from web scripts as `window.<name>(payload)`.
    pub async fn register_event_sink(&self, name: &str) -> Result<(), WebError> {
        self.inner.register_event_sink(name).await
    }

    /// Subscribe to web events triggered by registered event sinks.
    pub fn subscribe_events(&self) -> broadcast::Receiver<WebEvent> {
        self.inner.subscribe_events()
    }

    /// Diagnostic health query for this page target.
    pub async fn check_health(&self) -> Result<PageHealth, WebError> {
        self.inner.check_health().await
    }
}
