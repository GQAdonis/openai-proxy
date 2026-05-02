use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppState,
    codex::{BackendProfile, ResponseStreamEvent, build_chat_completions_request, convert_request, map_model},
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
                        "description": "Model to use (e.g. gpt-5.3-codex, codex-mini). Defaults to gpt-5.3-codex.",
                        "default": "gpt-5.3-codex"
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
    let messages_val = params
        .get("messages")
        .ok_or("missing required field: messages")?;

    let messages: Vec<Message> = serde_json::from_value(messages_val.clone())
        .map_err(|e| format!("invalid messages format: {e}"))?;

    let model_str = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("gpt-5.3-codex");

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

    let raw = match state.backend_profile {
        BackendProfile::OpenAiChatCompletions => {
            let outbound = build_chat_completions_request(&chat_req, state.default_model.as_deref());
            let (auth_header, _) = state.auth.bearer();
            let resp = state
                .http_client
                .post(&state.backend_url)
                .header("authorization", &auth_header)
                .header("content-type", "application/json")
                .json(&outbound)
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("upstream error {status}: {body}"));
            }
            let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
            return Ok(body["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string());
        }
        profile => {
            let codex_req = convert_request(&chat_req, state.default_model.as_deref(), profile);
            let (auth_header, account_id) = state.auth.bearer();
            let mut req_builder = state
                .http_client
                .post(&state.backend_url)
                .header("authorization", &auth_header)
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .header("accept-language", "en-US,en;q=0.9")
                .header("origin", "https://chatgpt.com")
                .header("referer", "https://chatgpt.com/")
                .header("sec-fetch-dest", "empty")
                .header("sec-fetch-mode", "cors")
                .header("sec-fetch-site", "same-origin")
                .header("openai-beta", "responses=experimental")
                .header("originator", "codex_cli_rs")
                .json(&codex_req);
            if let Some(id) = account_id {
                req_builder = req_builder.header("chatgpt-account-id", id);
            }
            let resp = req_builder.send().await.map_err(|e| format!("request failed: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("upstream error {status}: {body}"));
            }
            resp.text().await.map_err(|e| format!("read error: {e}"))?
        }
    };

    let mut parts: Vec<String> = Vec::new();
    for line in raw.lines() {
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data == "[DONE]" { break; }
        let Ok(event) = serde_json::from_str::<ResponseStreamEvent>(data) else { continue };
        if let ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } = event {
            parts.push(delta);
        }
    }

    Ok(parts.join(""))
}

fn list_models_tool(state: &AppState) -> String {
    match state.backend_profile {
        BackendProfile::ChatGptCodex => {
            "Available models (ChatGPT subscription):\n\
             • gpt-5.3-codex — Codex model (400K context)\n\
             • gpt-5.4 — GPT-5.4 (400K context)\n\
             • gpt-5.5 — GPT-5.5 (400K context)\n\
             \n\
             Aliases: gpt-4o, gpt-4, gpt-4-turbo, gpt-3.5-turbo → gpt-5.3-codex"
        }
        BackendProfile::OpenAiResponses => {
            "Available models (OpenAI Responses API):\n\
             • gpt-5.5 — GPT-5.5 (1M context)\n\
             • gpt-5.5-pro — GPT-5.5 Pro (Pro/Business/Enterprise only)\n\
             • gpt-5.4 — GPT-5.4\n\
             • gpt-5.3-codex — Codex model\n\
             • codex-mini — Faster, lighter Codex model"
        }
        BackendProfile::OpenAiChatCompletions => {
            "Available models (OpenAI Chat Completions):\n\
             • gpt-5.5 — GPT-5.5\n\
             • gpt-5.4 — GPT-5.4\n\
             • gpt-5.3-codex — Codex model\n\
             • codex-mini — Faster, lighter Codex model\n\
             • gpt-4o — GPT-4o\n\
             • gpt-4o-mini — GPT-4o Mini\n\
             • gpt-3.5-turbo — GPT-3.5 Turbo"
        }
    }.to_string()
}

fn check_auth_tool(state: &AppState) -> String {
    if state.auth.access_token.is_some() {
        format!(
            "✓ Authenticated via ChatGPT OAuth token (ChatGPT Plus/Pro subscription)\n\
             Backend: {}\n\
             Account ID: {}",
            state.backend_url,
            state
                .auth
                .account_id
                .as_deref()
                .unwrap_or("(not set)")
        )
    } else if state.auth.api_key.is_some() {
        format!(
            "✓ Authenticated via OpenAI API key\n\
             Backend: {}",
            state.backend_url
        )
    } else {
        "✗ No credentials loaded. Run `codex login` or set OPENAI_API_KEY.".to_string()
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
        ("gpt-5.3-codex", "deeper reasoning for complex tasks")
    } else {
        ("gpt-5.3-codex", "general-purpose default")
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
                "list_models" => (list_models_tool(state), false),
                "check_auth" => (check_auth_tool(state), false),
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
                let notif =
                    JsonRpcResponse::notification("notifications/initialized", serde_json::json!({}));
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
use tower_http::cors::CorsLayer;

pub async fn run_http(state: AppState, port: u16) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    run_http_on(state, listener).await
}

pub async fn run_http_on(state: AppState, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/mcp", post(mcp_http_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = listener.local_addr()?;
    tracing::info!("MCP Streamable HTTP server listening on http://{addr}/mcp");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn mcp_http_handler(
    AxumState(state): AxumState<AppState>,
    body: axum::body::Bytes,
) -> Result<AxumResponse, (StatusCode, String)> {
    let req: JsonRpcRequest = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Parse error: {e}")))?;

    match dispatch(&state, req).await {
        Some(resp) => {
            let json = serde_json::to_string(&resp)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((
                StatusCode::OK,
                [("content-type", "application/json")],
                json,
            )
                .into_response())
        }
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}
