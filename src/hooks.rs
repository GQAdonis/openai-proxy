//! Webhook-based hooks system for proxy lifecycle events.
//!
//! Provides the `ProxyHooks` trait, a no-op `NullHooks` default, and a
//! config-file-driven `WebhookHooks` implementation that POSTs AG-UI-compatible
//! JSON payloads to configured URLs on each event type.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// HookEvent
// ---------------------------------------------------------------------------

/// All proxy lifecycle events that can be fired.
#[derive(Debug, Clone)]
pub enum HookEvent {
    RequestReceived { model: String, message_count: usize },
    TextDelta { delta: String },
    ToolCallStart { name: String, call_id: String },
    ToolCallArgs { call_id: String, args_delta: String },
    ToolResultSubmitted { call_id: String },
    ResponseComplete { finish_reason: String },
    Error { status: u16, message: String },
}

impl HookEvent {
    /// Returns the event type string used as the TOML config key and in AG-UI payloads.
    pub fn event_type(&self) -> &'static str {
        match self {
            HookEvent::RequestReceived { .. } => "on_request_received",
            HookEvent::TextDelta { .. } => "on_text_delta",
            HookEvent::ToolCallStart { .. } => "on_tool_call_start",
            HookEvent::ToolCallArgs { .. } => "on_tool_call_args",
            HookEvent::ToolResultSubmitted { .. } => "on_tool_result_submitted",
            HookEvent::ResponseComplete { .. } => "on_response_complete",
            HookEvent::Error { .. } => "on_error",
        }
    }

    /// Serialise to an AG-UI-compatible JSON value.
    pub fn to_payload(&self) -> serde_json::Value {
        let timestamp = chrono_iso8601();
        match self {
            HookEvent::RequestReceived { model, message_count } => serde_json::json!({
                "type": "on_request_received",
                "timestamp": timestamp,
                "model": model,
                "message_count": message_count,
            }),
            HookEvent::TextDelta { delta } => serde_json::json!({
                "type": "on_text_delta",
                "timestamp": timestamp,
                "delta": delta,
            }),
            HookEvent::ToolCallStart { name, call_id } => serde_json::json!({
                "type": "on_tool_call_start",
                "timestamp": timestamp,
                "name": name,
                "call_id": call_id,
            }),
            HookEvent::ToolCallArgs { call_id, args_delta } => serde_json::json!({
                "type": "on_tool_call_args",
                "timestamp": timestamp,
                "call_id": call_id,
                "args_delta": args_delta,
            }),
            HookEvent::ToolResultSubmitted { call_id } => serde_json::json!({
                "type": "on_tool_result_submitted",
                "timestamp": timestamp,
                "call_id": call_id,
            }),
            HookEvent::ResponseComplete { finish_reason } => serde_json::json!({
                "type": "on_response_complete",
                "timestamp": timestamp,
                "finish_reason": finish_reason,
            }),
            HookEvent::Error { status, message } => serde_json::json!({
                "type": "on_error",
                "timestamp": timestamp,
                "status": status,
                "message": message,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// ProxyHooks trait
// ---------------------------------------------------------------------------

/// Async trait for proxy hook implementations.
///
/// Implementations must be object-safe so they can be stored as
/// `Arc<dyn ProxyHooks + Send + Sync>`. We use an explicit
/// `Pin<Box<dyn Future>>` return type to avoid the `async_trait` crate.
pub trait ProxyHooks: Send + Sync {
    /// Fire a hook event.  Implementations MUST NOT propagate errors to the caller.
    fn fire(&self, event: HookEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// NullHooks — no-op default
// ---------------------------------------------------------------------------

/// Default no-op implementation.  All hook fires are ignored.
pub struct NullHooks;

impl ProxyHooks for NullHooks {
    fn fire(&self, _event: HookEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

// ---------------------------------------------------------------------------
// HooksConfig — serde for hooks.toml
// ---------------------------------------------------------------------------

/// A single `[on_<event>]` section in `hooks.toml`.
#[derive(Debug, Deserialize)]
pub struct HookEndpoint {
    pub url: String,
}

/// Full `hooks.toml` schema.
#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    pub on_request_received: Option<HookEndpoint>,
    pub on_text_delta: Option<HookEndpoint>,
    pub on_tool_call_start: Option<HookEndpoint>,
    pub on_tool_call_args: Option<HookEndpoint>,
    pub on_tool_result_submitted: Option<HookEndpoint>,
    pub on_response_complete: Option<HookEndpoint>,
    pub on_error: Option<HookEndpoint>,
}

// ---------------------------------------------------------------------------
// WebhookHooks
// ---------------------------------------------------------------------------

/// Webhook-based hooks implementation.
///
/// Reads a `hooks.toml` config file at startup and POSTs AG-UI-compatible
/// JSON payloads to the configured URL for each event type.  Hook delivery
/// is fire-and-forget: errors are logged but never propagated to the caller.
pub struct WebhookHooks {
    /// Map from event-type string (e.g. `"on_text_delta"`) to webhook URL.
    urls: HashMap<String, String>,
    client: reqwest::Client,
}

impl WebhookHooks {
    /// Construct from an explicit URL map and a shared HTTP client.
    pub fn new(urls: HashMap<String, String>, client: reqwest::Client) -> Self {
        Self { urls, client }
    }

    /// Load from a `hooks.toml` file at `path`.
    pub fn from_config_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read hooks config {path}: {e}"))?;
        let config: HooksConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse hooks config {path}: {e}"))?;

        let mut urls = HashMap::new();
        macro_rules! insert_if_some {
            ($field:expr, $key:expr) => {
                if let Some(ep) = $field {
                    urls.insert($key.to_string(), ep.url);
                }
            };
        }
        insert_if_some!(config.on_request_received, "on_request_received");
        insert_if_some!(config.on_text_delta, "on_text_delta");
        insert_if_some!(config.on_tool_call_start, "on_tool_call_start");
        insert_if_some!(config.on_tool_call_args, "on_tool_call_args");
        insert_if_some!(config.on_tool_result_submitted, "on_tool_result_submitted");
        insert_if_some!(config.on_response_complete, "on_response_complete");
        insert_if_some!(config.on_error, "on_error");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build webhook http client: {e}"))?;

        Ok(Self { urls, client })
    }
}

impl ProxyHooks for WebhookHooks {
    fn fire(&self, event: HookEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let event_type = event.event_type().to_string();
        let url = self.urls.get(event_type.as_str()).cloned();
        let client = self.client.clone();

        Box::pin(async move {
            let Some(url) = url else { return };
            let payload = event.to_payload();
            tokio::spawn(async move {
                match client.post(&url).json(&payload).send().await {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            tracing::warn!(
                                event_type = %event_type,
                                url = %url,
                                status = %resp.status(),
                                "webhook returned non-2xx status"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            event_type = %event_type,
                            url = %url,
                            error = %e,
                            "webhook POST failed"
                        );
                    }
                }
            });
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the current time as an ISO 8601 string without pulling in the
/// `chrono` crate.  Uses `SystemTime`.
fn chrono_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format as RFC 3339 / ISO 8601 UTC.
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Approximate calendar conversion (good enough for log timestamps).
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

// ---------------------------------------------------------------------------
// Serialise HookEvent (needed for WebhookHooks payload building above)
// ---------------------------------------------------------------------------

impl Serialize for HookEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_payload().serialize(serializer)
    }
}
