use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppState,
    codex::{
        BackendProfile, ResponseStreamEvent, build_chat_completions_request, convert_request,
        map_model,
    },
    openai::{ChatCompletionRequest, Message},
};

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }

    fn notification(method: &str, params: Value) -> String {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        serde_json::to_string(&notif).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// MCP tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    serde_json::json!([
        {
            "name": "chat_completion",
            "description": "Send a chat completion request through the openai-proxy to the Codex backend using your ChatGPT subscription. Returns the assistant's response text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "model": {
                        "type": "string",
                        "description": "Model to use (e.g. gpt-6.1-sol, gpt-5.6-sol). Defaults to gpt-6.1-sol.",
                        "default": "gpt-6.1-sol"
                    },
                    "messages": {
                        "type": "array",
                        "description": "Conversation messages in OpenAI format.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "role": { "type": "string", "enum": ["system", "user", "assistant"] },
                                "content": { "type": "string" }
                            },
                            "required": ["role", "content"]
                        }
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum tokens to generate."
                    },
                    "temperature": {
                        "type": "number",
                        "description": "Sampling temperature (0.0–2.0)."
                    }
                },
                "required": ["messages"]
            }
        },
        {
            "name": "list_models",
            "description": "List available Codex models that can be used with this proxy.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "check_auth",
            "description": "Check whether valid Codex credentials are loaded. Returns the auth status and which backend will be used.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "set_model",
            "description": "Get a recommendation for which model to use based on your task. Does not persist state — this is advisory only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Describe your task to get a model recommendation (e.g. 'code generation', 'quick question', 'complex reasoning')."
                    }
                },
                "required": ["task"]
            }
        }
    ])
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

async fn chat_completion_tool(state: &AppState, params: &Value) -> Result<String, String> {
    let state = state.for_request().await.map_err(|e| e.to_string())?;
    let messages_val = params
        .get("messages")
        .ok_or("missing required field: messages")?;

    let messages: Vec<Message> = serde_json::from_value(messages_val.clone())
        .map_err(|e| format!("invalid messages format: {e}"))?;

    let model_str = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("gpt-6.1-sol");

    // Apply default model override from state if the client didn't specify explicitly.
    let effective_model = if params.get("model").is_none() {
        state
            .default_model
            .as_deref()
            .unwrap_or(model_str)
            .to_string()
    } else {
        map_model(model_str).to_string()
    };

    let max_tokens: Option<u32> = params
        .get("max_tokens")
        .and_then(Value::as_u64)
        .map(|v| v as u32);

    let temperature: Option<f32> = params
        .get("temperature")
        .and_then(Value::as_f64)
        .map(|v| v as f32);

    let chat_req = ChatCompletionRequest {
        model: effective_model,
        messages,
        stream: false,
        max_tokens,
        temperature,
        top_p: None,
        stop: None,
        system: None,
        user: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
    };

    let retry_safe = chat_req
        .messages
        .iter()
        .all(|m| m.role != "tool" && m.tool_calls.is_none());
    let raw = match state.backend_profile {
        BackendProfile::OpenAiChatCompletions => {
            let outbound =
                build_chat_completions_request(&chat_req, state.default_model.as_deref());
            let (auth_header, _) = state.auth.bearer().map_err(|e| e.to_string())?;
            let builder = state
                .http_client
                .post(&state.backend_url)
                .header("authorization", &auth_header)
                .header("content-type", "application/json")
                .json(&outbound);
            let resp = state
                .send_upstream(builder, retry_safe)
                .await
                .map_err(|e| format!("request failed: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let body = crate::upstream_error(resp.status());
                return Err(format!("upstream error {status}: {body}"));
            }
            let body: serde_json::Value =
                resp.json().await.map_err(|e| format!("parse error: {e}"))?;
            return Ok(body["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string());
        }
        profile => {
            let codex_req = convert_request(&chat_req, state.default_model.as_deref(), profile);
            let request = crate::proxy::build_responses_request(&state, &codex_req)
                .map_err(|e| e.to_string())?;
            let resp = state
                .send_upstream(request, retry_safe)
                .await
                .map_err(|e| format!("request failed: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let body = crate::upstream_error(resp.status());
                return Err(format!("upstream error {status}: {body}"));
            }
            resp.text().await.map_err(|e| format!("read error: {e}"))?
        }
    };

    let mut parts: Vec<String> = Vec::new();
    for line in raw.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let Ok(event) = serde_json::from_str::<ResponseStreamEvent>(data) else {
            continue;
        };
        if let ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } = event {
            parts.push(delta);
        }
    }

    Ok(parts.join(""))
}

async fn list_models_tool(state: &AppState) -> Result<String, String> {
    let profile = state
        .credentials
        .snapshot()
        .await
        .map_err(|e| e.to_string())?
        .backend_profile;
    let names = crate::model_catalog::CATALOG
        .iter()
        .filter(|e| e.supports(profile))
        .map(|e| e.model_id)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("Available models ({}): {names}", profile.name()))
}

async fn check_auth_tool(state: &AppState) -> String {
    let status = state.credentials.status().await;
    if matches!(status.state, "ready" | "reused") {
        format!("✓ Authenticated ({})", status.mode)
    } else {
        format!("Connection {} ({})", status.state, status.mode)
    }
}

fn set_model_tool(params: &Value) -> String {
    let task = params
        .get("task")
        .and_then(Value::as_str)
        .unwrap_or("general");

    let task_lower = task.to_lowercase();
    let (recommended, reason) = if task_lower.contains("quick")
        || task_lower.contains("fast")
        || task_lower.contains("simple")
        || task_lower.contains("short")
    {
        ("codex-mini", "faster responses for simple tasks")
    } else if task_lower.contains("complex")
        || task_lower.contains("reason")
        || task_lower.contains("architect")
        || task_lower.contains("design")
        || task_lower.contains("long")
    {
        ("gpt-5.6-sol", "deeper reasoning for complex tasks")
    } else {
        ("gpt-6.1-sol", "general-purpose default")
    };

    format!(
        "Recommended model for '{task}': {recommended}\nReason: {reason}\n\
         \nTo use this model, set it in your request or set CODEX_DEFAULT_MODEL={recommended}"
    )
}

// ---------------------------------------------------------------------------
// Core dispatch (shared by stdio and HTTP transports)
// ---------------------------------------------------------------------------

pub async fn dispatch(state: &AppState, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    match req.method.as_str() {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "openai-proxy",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            Some(JsonRpcResponse::ok(req.id, result))
        }

        "notifications/initialized" => None,

        "ping" => Some(JsonRpcResponse::ok(req.id, serde_json::json!({}))),

        "tools/list" => {
            let result = serde_json::json!({ "tools": tool_definitions() });
            Some(JsonRpcResponse::ok(req.id, result))
        }

        "tools/call" => {
            let name = req.params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let (text, is_error) = match name {
                "chat_completion" => match chat_completion_tool(state, &args).await {
                    Ok(t) => (t, false),
                    Err(e) => (e, true),
                },
                "list_models" => match list_models_tool(state).await {
                    Ok(text) => (text, false),
                    Err(error) => (error, true),
                },
                "check_auth" => (check_auth_tool(state).await, false),
                "set_model" => (set_model_tool(&args), false),
                other => (format!("Unknown tool: {other}"), true),
            };

            let result = serde_json::json!({
                "content": [{ "type": "text", "text": text }],
                "isError": is_error
            });
            Some(JsonRpcResponse::ok(req.id, result))
        }

        other => Some(JsonRpcResponse::err(
            req.id,
            -32601,
            format!("Method not found: {other}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// stdio transport
// ---------------------------------------------------------------------------

pub async fn run_stdio(state: AppState) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("PROXY_CONSOLE_PRINCIPALS").is_none(),
        "MCP stdio requires a dedicated credential owner"
    );
    tracing::info!("MCP stdio transport started — reading JSON-RPC from stdin");

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = JsonRpcResponse::err(None, -32700, format!("Parse error: {e}"));
                let json = serde_json::to_string(&err_resp)?;
                writeln!(stdout.lock(), "{json}")?;
                stdout.lock().flush()?;
                continue;
            }
        };

        // `initialize` triggers an `initialized` notification before the response.
        let is_init = req.method == "initialize";
        let id = req.id.clone();

        if let Some(resp) = dispatch(&state, req).await {
            let json = serde_json::to_string(&resp)?;
            writeln!(stdout.lock(), "{json}")?;

            if is_init {
                let notif = JsonRpcResponse::notification(
                    "notifications/initialized",
                    serde_json::json!({}),
                );
                writeln!(stdout.lock(), "{notif}")?;
            }

            stdout.lock().flush()?;
        }

        let _ = id;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Streamable HTTP transport
// ---------------------------------------------------------------------------

use axum::{
    Router,
    extract::State as AxumState,
    http::StatusCode,
    response::{IntoResponse, Response as AxumResponse},
    routing::post,
};

pub async fn run_http(state: AppState, port: u16) -> anyhow::Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    run_http_on(state, listener).await
}

pub async fn run_http_on(state: AppState, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("PROXY_CONSOLE_PRINCIPALS").is_none()
            && listener.local_addr()?.ip().is_loopback(),
        "standalone MCP requires a dedicated loopback owner"
    );
    let app = Router::new()
        .route("/mcp", post(mcp_http_handler))
        .with_state(state);

    let addr = listener.local_addr()?;
    tracing::info!("MCP Streamable HTTP server listening on http://{addr}/mcp");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn mcp_http_handler(
    AxumState(state): AxumState<AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<AxumResponse, (StatusCode, String)> {
    if let Some(secret) = std::env::var_os("PROXY_MCP_TOKEN") {
        use sha2::{Digest, Sha256};
        use subtle::ConstantTimeEq;
        let candidate = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let Some(expected) = secret.to_str().filter(|s| s.len() >= 32 && s.len() <= 512) else {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "local MCP credential configuration invalid".into(),
            ));
        };
        if !candidate.filter(|s| s.len() <= 512).is_some_and(|s| {
            bool::from(
                Sha256::digest(s.as_bytes())
                    .as_slice()
                    .ct_eq(Sha256::digest(expected.as_bytes()).as_slice()),
            )
        }) {
            return Err((StatusCode::UNAUTHORIZED, "MCP bearer required".into()));
        }
    }
    if headers.contains_key(axum::http::header::ORIGIN) {
        return Err((
            StatusCode::FORBIDDEN,
            "browser origins are not authorized on this local client transport".into(),
        ));
    }
    let req: JsonRpcRequest = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Parse error: {e}")))?;

    match dispatch(&state, req).await {
        Some(resp) => {
            let json = serde_json::to_string(&resp)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((StatusCode::OK, [("content-type", "application/json")], json).into_response())
        }
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}
