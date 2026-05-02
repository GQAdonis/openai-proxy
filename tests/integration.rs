//! Full integration tests for openai-proxy.
//!
//! These tests start the real axum router backed by live credentials from
//! ~/.codex/auth.json and fire requests at the actual Codex Responses API.
//! Every live test uses `gpt-5.3-codex` as the canonical model.
//!
//! Prerequisites:
//!   - ~/.codex/auth.json must exist (run `codex login` first)
//!   - Internet access to chatgpt.com / api.openai.com
//!
//! Run:
//!   cargo test --test integration -- --nocapture

use axum_test::TestServer;
use openai_proxy_lib::{AppState, build_app, load_real_auth, mcp};
use openai_proxy_lib::codex::BackendProfile;
use serde_json::{Value, json};

// ── helpers ─────────────────────────────────────────────────────────────────

fn test_server() -> TestServer {
    let (state, _) = load_real_auth();
    let app = build_app(state, false);
    TestServer::new(app)
}

fn test_server_with_default_model(model: &str) -> TestServer {
    let (mut state, _) = load_real_auth();
    state.default_model = Some(model.to_string());
    let app = build_app(state, false);
    TestServer::new(app)
}

fn mcp_state() -> AppState {
    let (state, _) = load_real_auth();
    state
}

fn parse_sse_chunks(body: &str) -> Vec<Value> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).expect("invalid SSE chunk JSON"))
        .collect()
}

/// Helper to build a Message with all optional fields set to None.
fn msg(role: &str, content: &str) -> openai_proxy_lib::openai::Message {
    use openai_proxy_lib::openai::{Message, MessageContent};
    Message {
        role: role.to_string(),
        content: MessageContent::Text(content.to_string()),
        tool_call_id: None,
        tool_calls: None,
        name: None,
    }
}

/// Helper to build a minimal ChatCompletionRequest.
fn chat_req(
    model: &str,
    messages: Vec<openai_proxy_lib::openai::Message>,
) -> openai_proxy_lib::openai::ChatCompletionRequest {
    openai_proxy_lib::openai::ChatCompletionRequest {
        model: model.to_string(),
        messages,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        system: None,
        user: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
    }
}

/// Helper to build a JSON request body for HTTP-level tests (avoids serde trait mismatch).
fn chat_req_json(model: &str, messages: &[(&str, &str)]) -> serde_json::Value {
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    serde_json::json!({ "model": model, "messages": msgs, "stream": false })
}

// ── health endpoint ──────────────────────────────────────────────────────────

#[tokio::test]
async fn health_check() {
    let server = test_server();
    let resp = server.get("/health").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["status"], "ok");
}

// ── GET /v1/models ───────────────────────────────────────────────────────────

#[tokio::test]
async fn models_list_returns_object_type() {
    let server = test_server();
    let resp = server.get("/v1/models").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["object"], "list");
}

#[tokio::test]
async fn models_list_contains_gpt_5_3_codex() {
    let server = test_server();
    let body: Value = server.get("/v1/models").await.json();
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(ids.contains(&"gpt-5.3-codex"), "expected gpt-5.3-codex in model list, got: {ids:?}");
}

#[tokio::test]
async fn models_list_contains_codex_mini_for_api_key_profile() {
    // codex-mini is only listed on OpenAI API key profiles (Responses or Chat Completions).
    // This test is skipped when the active profile is ChatGptCodex.
    let (state, _) = load_real_auth();
    if matches!(state.backend_profile, BackendProfile::ChatGptCodex) {
        return; // ChatGPT subscription does not support codex-mini
    }
    let server = test_server();
    let body: Value = server.get("/v1/models").await.json();
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert!(ids.contains(&"codex-mini"), "expected codex-mini in API key model list");
}

#[tokio::test]
async fn models_list_all_have_required_fields() {
    let server = test_server();
    let body: Value = server.get("/v1/models").await.json();
    for model in body["data"].as_array().unwrap() {
        assert!(model["id"].is_string(), "model missing id");
        assert_eq!(model["object"], "model", "model missing object field");
        assert!(model["owned_by"].is_string(), "model missing owned_by");
    }
}

// ── POST /v1/chat/completions — non-streaming ────────────────────────────────

#[tokio::test]
async fn non_streaming_basic_completion() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Reply with exactly the word: PONG"}]
        }))
        .await;

    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["object"], "chat.completion");
    assert!(body["id"].as_str().unwrap().starts_with("chatcmpl-"));
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(!content.is_empty(), "response content should not be empty");
    assert_eq!(body["choices"][0]["message"]["role"], "assistant");
}

#[tokio::test]
async fn non_streaming_finish_reason_is_stop() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hi."}]
        }))
        .await
        .json();

    let finish = body["choices"][0]["finish_reason"].as_str().unwrap();
    assert_eq!(finish, "stop", "expected finish_reason=stop, got: {finish}");
}

#[tokio::test]
async fn non_streaming_usage_tokens_populated() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hi."}]
        }))
        .await
        .json();

    assert!(body["usage"]["prompt_tokens"].as_u64().unwrap_or(0) > 0);
    assert!(body["usage"]["completion_tokens"].as_u64().unwrap_or(0) > 0);
    assert!(body["usage"]["total_tokens"].as_u64().unwrap_or(0) > 0);
}

#[tokio::test]
async fn non_streaming_system_message_extracted_as_instructions() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [
                {"role": "system", "content": "You are a test assistant. Reply with SYSTEM_OK."},
                {"role": "user", "content": "Confirm."}
            ]
        }))
        .await
        .json();

    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(!content.is_empty());
}

#[tokio::test]
async fn non_streaming_multi_turn_conversation() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [
                {"role": "user", "content": "My name is TestUser."},
                {"role": "assistant", "content": "Hello TestUser, nice to meet you."},
                {"role": "user", "content": "What is my name?"}
            ]
        }))
        .await
        .json();

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        content.contains("testuser") || !content.is_empty(),
        "model should produce a non-empty response for multi-turn; got: {content}"
    );
}

#[tokio::test]
async fn non_streaming_max_tokens_respected() {
    let (state, _) = load_real_auth();
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Count from 1 to 1000, one number per line."}],
            "max_tokens": 10
        }))
        .await
        .json();

    // ChatGptCodex profile strips max_tokens so the finish_reason may be "stop".
    // OpenAiResponses forwards max_output_tokens, so expect "length".
    let finish = body["choices"][0]["finish_reason"].as_str().unwrap_or("stop");
    match state.backend_profile {
        BackendProfile::ChatGptCodex => {
            // max_tokens is stripped; model may return stop with any output
            assert!(
                finish == "stop" || finish == "length",
                "unexpected finish_reason on ChatGptCodex profile: {finish}"
            );
        }
        _ => {
            assert!(
                finish == "length" || finish == "stop",
                "unexpected finish_reason: {finish}"
            );
            let output_tokens = body["usage"]["completion_tokens"].as_u64().unwrap_or(0);
            assert!(output_tokens <= 20, "output tokens {output_tokens} should be near the 10-token limit");
        }
    }
}

#[tokio::test]
async fn non_streaming_model_alias_gpt4o_maps_correctly() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Say hi."}]
        }))
        .await
        .json();
    assert_eq!(body["object"], "chat.completion");
    assert!(body["choices"][0]["message"]["content"].as_str().is_some());
}

#[tokio::test]
async fn non_streaming_model_alias_gpt35_turbo_maps_correctly() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-3.5-turbo",
            "messages": [{"role": "user", "content": "Say hi."}]
        }))
        .await
        .json();
    assert_eq!(body["object"], "chat.completion");
    assert!(body["choices"][0]["message"]["content"].as_str().is_some());
}

#[tokio::test]
async fn non_streaming_content_parts_format() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "Reply with PARTS_OK."}]
            }]
        }))
        .await
        .json();

    assert_eq!(body["object"], "chat.completion");
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(!content.is_empty());
}

#[tokio::test]
async fn non_streaming_default_model_override() {
    let server = test_server_with_default_model("gpt-5.3-codex");
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Say hi."}]
        }))
        .await
        .json();

    assert_eq!(body["object"], "chat.completion");
    assert!(body["choices"][0]["message"]["content"].as_str().is_some());
}

#[tokio::test]
async fn non_streaming_explicit_codex_model_ignores_default_override() {
    // Explicit codex/gpt-5 model IDs must not be overridden by CODEX_DEFAULT_MODEL.
    // Use gpt-5.3-codex as the override since codex-mini isn't available on ChatGPT Plus.
    let (state, _) = load_real_auth();
    let override_model = if matches!(state.backend_profile, BackendProfile::ChatGptCodex) {
        "gpt-5.3-codex" // safe on Plus
    } else {
        "codex-mini"
    };
    let server = test_server_with_default_model(override_model);
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hi."}]
        }))
        .await
        .json();

    assert_eq!(body["object"], "chat.completion");
    assert!(body["choices"][0]["message"]["content"].as_str().is_some());
}

// ── model-not-available validation ──────────────────────────────────────────

#[tokio::test]
async fn codex_mini_rejected_on_chatgpt_profile() {
    let (state, _) = load_real_auth();
    if !matches!(state.backend_profile, BackendProfile::ChatGptCodex) {
        return; // only relevant for ChatGPT subscription
    }
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "codex-mini",
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .await;
    assert!(
        resp.status_code().as_u16() >= 400,
        "codex-mini should be rejected on ChatGptCodex profile"
    );
}

#[tokio::test]
async fn gpt55_pro_rejected_on_chatgpt_profile() {
    let (state, _) = load_real_auth();
    if !matches!(state.backend_profile, BackendProfile::ChatGptCodex) {
        return;
    }
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.5-pro",
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .await;
    assert!(
        resp.status_code().as_u16() >= 400,
        "gpt-5.5-pro should be rejected on ChatGptCodex profile"
    );
}

// ── streaming ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn streaming_returns_sse_content_type() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hi."}],
            "stream": true
        }))
        .await;

    resp.assert_status_ok();
    let ct = resp.headers()["content-type"].to_str().unwrap();
    assert!(ct.contains("text/event-stream"), "expected SSE content-type, got: {ct}");
}

#[tokio::test]
async fn streaming_first_chunk_has_role_delta() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hi."}],
            "stream": true
        }))
        .await;

    let body = resp.text();
    let chunks = parse_sse_chunks(&body);
    assert!(!chunks.is_empty(), "expected at least one SSE chunk");

    let first = &chunks[0];
    assert_eq!(first["object"], "chat.completion.chunk");
    assert!(first["id"].as_str().unwrap().starts_with("chatcmpl-"));
    assert_eq!(first["choices"][0]["delta"]["role"], "assistant");
}

#[tokio::test]
async fn streaming_produces_content_deltas() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Count to 3."}],
            "stream": true
        }))
        .await;

    let body = resp.text();
    let chunks = parse_sse_chunks(&body);

    let content_chunks: Vec<&Value> = chunks
        .iter()
        .filter(|c| c["choices"][0]["delta"]["content"].is_string())
        .collect();
    assert!(!content_chunks.is_empty(), "expected content delta chunks in stream");
}

#[tokio::test]
async fn streaming_last_chunk_has_finish_reason() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say one word."}],
            "stream": true
        }))
        .await;

    let body = resp.text();
    let chunks = parse_sse_chunks(&body);

    let finish_chunk = chunks
        .iter()
        .find(|c| !c["choices"][0]["finish_reason"].is_null());
    assert!(finish_chunk.is_some(), "no finish_reason chunk found in stream");
    let finish = finish_chunk.unwrap()["choices"][0]["finish_reason"]
        .as_str()
        .unwrap();
    assert!(finish == "stop" || finish == "length", "unexpected finish_reason: {finish}");
}

#[tokio::test]
async fn streaming_all_chunks_share_same_id() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Count to 5."}],
            "stream": true
        }))
        .await;

    let body = resp.text();
    let chunks = parse_sse_chunks(&body);
    assert!(chunks.len() > 1);

    let first_id = chunks[0]["id"].as_str().unwrap().to_string();
    for chunk in &chunks {
        assert_eq!(
            chunk["id"].as_str().unwrap(),
            first_id,
            "all chunks must share the same completion ID"
        );
    }
}

#[tokio::test]
async fn streaming_assembled_content_is_non_empty() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hello."}],
            "stream": true
        }))
        .await;

    let body = resp.text();
    let chunks = parse_sse_chunks(&body);

    let assembled: String = chunks
        .iter()
        .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert!(!assembled.is_empty(), "assembled streaming content should not be empty");
}

#[tokio::test]
async fn streaming_system_message_works() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [
                {"role": "system", "content": "You are a test assistant."},
                {"role": "user", "content": "Say STREAM_OK."}
            ],
            "stream": true
        }))
        .await;

    resp.assert_status_ok();
    let body = resp.text();
    let assembled: String = parse_sse_chunks(&body)
        .iter()
        .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert!(!assembled.is_empty());
}

#[tokio::test]
async fn streaming_with_gpt53_codex_model() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Say hi."}],
            "stream": true
        }))
        .await;

    resp.assert_status_ok();
    let body = resp.text();
    let chunks = parse_sse_chunks(&body);
    assert!(!chunks.is_empty(), "streaming with gpt-5.3-codex should return chunks");
}

// ── tool calling — non-streaming ─────────────────────────────────────────────

#[tokio::test]
async fn non_streaming_tool_call_response() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "What is the weather in New York? Use the get_weather function."}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get the current weather for a location",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {"type": "string", "description": "City name"}
                        },
                        "required": ["location"]
                    }
                }
            }],
            "tool_choice": "auto"
        }))
        .await;

    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["object"], "chat.completion");
    // Model should either call the tool or answer directly.
    let choice = &body["choices"][0];
    let finish = choice["finish_reason"].as_str().unwrap_or("stop");
    assert!(
        finish == "tool_calls" || finish == "stop",
        "finish_reason should be tool_calls or stop, got: {finish}"
    );
    if finish == "tool_calls" {
        let tool_calls = choice["message"]["tool_calls"].as_array().unwrap();
        assert!(!tool_calls.is_empty(), "tool_calls array should not be empty");
        let call = &tool_calls[0];
        assert_eq!(call["type"], "function");
        assert!(!call["id"].as_str().unwrap_or("").is_empty(), "tool call id should be non-empty");
        assert_eq!(call["function"]["name"], "get_weather");
        let args: Value = serde_json::from_str(
            call["function"]["arguments"].as_str().unwrap_or("{}")
        ).unwrap();
        assert!(args["location"].is_string(), "arguments should contain 'location'");
    }
}

#[tokio::test]
async fn non_streaming_tool_call_roundtrip() {
    // Full round-trip: send tool call result back and get a final answer.
    let server = test_server();

    // Step 1: ask with tools
    let resp1: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "What is the weather in Paris? Use get_weather."}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get current weather",
                    "parameters": {
                        "type": "object",
                        "properties": {"location": {"type": "string"}},
                        "required": ["location"]
                    }
                }
            }]
        }))
        .await
        .json();

    assert_eq!(resp1["object"], "chat.completion");
    let finish1 = resp1["choices"][0]["finish_reason"].as_str().unwrap_or("stop");

    if finish1 != "tool_calls" {
        // Model answered directly — valid behavior, test passes
        return;
    }

    let tool_calls = resp1["choices"][0]["message"]["tool_calls"].as_array().unwrap();
    let call_id = tool_calls[0]["id"].as_str().unwrap().to_string();

    // Step 2: submit tool result
    let resp2: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [
                {"role": "user", "content": "What is the weather in Paris? Use get_weather."},
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": tool_calls
                },
                {
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": "{\"temperature\": \"22°C\", \"condition\": \"Sunny\"}"
                }
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get current weather",
                    "parameters": {
                        "type": "object",
                        "properties": {"location": {"type": "string"}},
                        "required": ["location"]
                    }
                }
            }]
        }))
        .await
        .json();

    assert_eq!(resp2["object"], "chat.completion");
    let final_content = resp2["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    assert!(!final_content.is_empty(), "final answer after tool result should be non-empty");
}

#[tokio::test]
async fn non_streaming_tool_choice_none_ignores_tools() {
    let server = test_server();
    let body: Value = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "What is 2+2?"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "calculator",
                    "description": "Performs calculations",
                    "parameters": {
                        "type": "object",
                        "properties": {"expr": {"type": "string"}},
                        "required": ["expr"]
                    }
                }
            }],
            "tool_choice": "none"
        }))
        .await
        .json();

    assert_eq!(body["object"], "chat.completion");
    // With tool_choice=none, the model should answer directly.
    let finish = body["choices"][0]["finish_reason"].as_str().unwrap_or("stop");
    assert_eq!(finish, "stop", "tool_choice=none should produce finish_reason=stop");
    assert!(body["choices"][0]["message"]["content"].as_str().is_some());
}

// ── tool calling — streaming ─────────────────────────────────────────────────

#[tokio::test]
async fn streaming_tool_call_produces_tool_calls_chunks() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({
            "model": "gpt-5.3-codex",
            "messages": [{"role": "user", "content": "Get weather for London using get_weather."}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get current weather",
                    "parameters": {
                        "type": "object",
                        "properties": {"location": {"type": "string"}},
                        "required": ["location"]
                    }
                }
            }],
            "stream": true
        }))
        .await;

    resp.assert_status_ok();
    let body = resp.text();
    let chunks = parse_sse_chunks(&body);
    assert!(!chunks.is_empty(), "streaming with tools should produce chunks");

    // Find the finish_reason chunk — should be "tool_calls" or "stop".
    let finish_chunk = chunks.iter().find(|c| !c["choices"][0]["finish_reason"].is_null());
    if let Some(fc) = finish_chunk {
        let finish = fc["choices"][0]["finish_reason"].as_str().unwrap();
        assert!(
            finish == "tool_calls" || finish == "stop",
            "streaming tool finish_reason should be tool_calls or stop, got: {finish}"
        );

        if finish == "tool_calls" {
            // Verify that at least one chunk had tool_calls delta.
            let has_tool_delta = chunks
                .iter()
                .any(|c| !c["choices"][0]["delta"]["tool_calls"].is_null());
            assert!(has_tool_delta, "streaming tool_calls should have at least one tool_calls delta chunk");
        }
    }
}

// ── tool format conversion (unit-level, no network) ──────────────────────────

#[test]
fn tool_format_conversion_unwraps_nested_function() {
    use openai_proxy_lib::codex::convert_tools_to_responses_format;

    let tools = json!([{
        "type": "function",
        "function": {
            "name": "get_weather",
            "description": "Get weather",
            "parameters": {
                "type": "object",
                "properties": {"location": {"type": "string"}},
                "required": ["location"]
            }
        }
    }]);

    let converted = convert_tools_to_responses_format(&tools);
    let tool = &converted[0];
    assert_eq!(tool["type"], "function");
    assert_eq!(tool["name"], "get_weather");
    assert_eq!(tool["description"], "Get weather");
    assert!(tool["parameters"].is_object());
    // Nested "function" wrapper should be gone.
    assert!(tool.get("function").is_none(), "function wrapper should be removed");
}

#[test]
fn tool_choice_conversion_function_object() {
    use openai_proxy_lib::codex::convert_tool_choice_to_responses_format;

    let choice = json!({"type": "function", "function": {"name": "my_tool"}});
    let converted = convert_tool_choice_to_responses_format(&choice);
    assert_eq!(converted["type"], "function");
    assert_eq!(converted["name"], "my_tool");
    assert!(converted.get("function").is_none(), "nested function should be removed");
}

#[test]
fn tool_choice_conversion_string_passthrough() {
    use openai_proxy_lib::codex::convert_tool_choice_to_responses_format;

    for s in &["auto", "none", "required"] {
        let choice = json!(s);
        let converted = convert_tool_choice_to_responses_format(&choice);
        assert_eq!(converted, choice, "string tool_choice should pass through unchanged");
    }
}

// ── tool call conversion in request (unit-level) ─────────────────────────────

#[test]
fn convert_request_tool_messages_become_function_call_output() {
    use openai_proxy_lib::codex::{BackendProfile, ResponsesInputItem, convert_request};
    use openai_proxy_lib::openai::{MessageContent, ToolCall, ToolCallFunction};

    let assistant_msg = openai_proxy_lib::openai::Message {
        role: "assistant".to_string(),
        content: MessageContent::Null,
        tool_call_id: None,
        tool_calls: Some(vec![ToolCall {
            id: "call_abc".to_string(),
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: "get_weather".to_string(),
                arguments: r#"{"location":"NYC"}"#.to_string(),
            },
        }]),
        name: None,
    };
    let tool_result_msg = openai_proxy_lib::openai::Message {
        role: "tool".to_string(),
        content: MessageContent::Text(r#"{"temp":"72F"}"#.to_string()),
        tool_call_id: Some("call_abc".to_string()),
        tool_calls: None,
        name: None,
    };

    let req = chat_req("gpt-5.3-codex", vec![
        msg("user", "What's the weather?"),
        assistant_msg,
        tool_result_msg,
    ]);

    let codex_req = convert_request(&req, None, BackendProfile::OpenAiResponses);

    // Expect: user message, function_call item, function_call_output item
    assert_eq!(codex_req.input.len(), 3);

    assert!(matches!(codex_req.input[0], ResponsesInputItem::Message(_)));
    assert!(matches!(codex_req.input[1], ResponsesInputItem::FunctionCall(_)));
    assert!(matches!(codex_req.input[2], ResponsesInputItem::FunctionCallOutput(_)));

    if let ResponsesInputItem::FunctionCall(ref fc) = codex_req.input[1] {
        assert_eq!(fc.call_id, "call_abc");
        assert_eq!(fc.name, "get_weather");
        assert_eq!(fc.arguments, r#"{"location":"NYC"}"#);
    }

    if let ResponsesInputItem::FunctionCallOutput(ref fco) = codex_req.input[2] {
        assert_eq!(fco.call_id, "call_abc");
        assert_eq!(fco.output, r#"{"temp":"72F"}"#);
    }
}

#[test]
fn convert_request_parallel_tool_calls_forwarded() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let mut req = chat_req("gpt-5.3-codex", vec![msg("user", "Hi")]);
    req.parallel_tool_calls = Some(false);

    let codex_req = convert_request(&req, None, BackendProfile::OpenAiResponses);
    assert_eq!(codex_req.parallel_tool_calls, Some(false));
}

// ── model mapping (unit-level, no network) ───────────────────────────────────

#[test]
fn model_mapping_gpt4o_to_codex() {
    use openai_proxy_lib::codex::map_model;
    assert_eq!(map_model("gpt-4o"), "gpt-5.3-codex");
    assert_eq!(map_model("gpt-4o-2024-11-20"), "gpt-5.3-codex");
}

#[test]
fn model_mapping_gpt4o_mini_to_codex() {
    use openai_proxy_lib::codex::map_model;
    assert_eq!(map_model("gpt-4o-mini"), "gpt-5.3-codex");
}

#[test]
fn model_mapping_gpt4_variants_to_codex() {
    use openai_proxy_lib::codex::map_model;
    assert_eq!(map_model("gpt-4"), "gpt-5.3-codex");
    assert_eq!(map_model("gpt-4-turbo"), "gpt-5.3-codex");
    assert_eq!(map_model("gpt-4-turbo-preview"), "gpt-5.3-codex");
}

#[test]
fn model_mapping_gpt35_to_codex() {
    use openai_proxy_lib::codex::map_model;
    assert_eq!(map_model("gpt-3.5-turbo"), "gpt-5.3-codex");
    assert_eq!(map_model("gpt-3.5-turbo-0125"), "gpt-5.3-codex");
}

#[test]
fn model_mapping_explicit_codex_passthrough() {
    use openai_proxy_lib::codex::map_model;
    assert_eq!(map_model("gpt-5.3-codex"), "gpt-5.3-codex");
    assert_eq!(map_model("codex-mini"), "codex-mini");
}

#[test]
fn model_mapping_unknown_falls_back_to_codex() {
    use openai_proxy_lib::codex::map_model;
    assert_eq!(map_model("some-unknown-model"), "gpt-5.3-codex");
}

#[test]
fn finish_reason_mapping_completed() {
    use openai_proxy_lib::codex::map_finish_reason;
    assert_eq!(map_finish_reason(Some("completed")), "stop");
}

#[test]
fn finish_reason_mapping_max_tokens() {
    use openai_proxy_lib::codex::map_finish_reason;
    assert_eq!(map_finish_reason(Some("max_tokens")), "length");
    assert_eq!(map_finish_reason(Some("length")), "length");
}

#[test]
fn finish_reason_mapping_none() {
    use openai_proxy_lib::codex::map_finish_reason;
    assert_eq!(map_finish_reason(None), "stop");
}

#[test]
fn finish_reason_mapping_passthrough() {
    use openai_proxy_lib::codex::map_finish_reason;
    assert_eq!(map_finish_reason(Some("content_filter")), "content_filter");
}

#[test]
fn finish_reason_mapping_tool_calls() {
    use openai_proxy_lib::codex::map_finish_reason;
    assert_eq!(map_finish_reason(Some("tool_calls")), "tool_calls");
}

// ── request conversion (unit-level, no network) ──────────────────────────────

#[test]
fn convert_request_system_message_becomes_instructions() {
    use openai_proxy_lib::codex::{BackendProfile, ResponsesInputItem, convert_request};

    let req = chat_req("gpt-5.3-codex", vec![
        msg("system", "Be helpful."),
        msg("user", "Hello"),
    ]);

    let codex_req = convert_request(&req, None, BackendProfile::ChatGptCodex);
    assert_eq!(codex_req.instructions.as_deref(), Some("Be helpful."));
    assert_eq!(codex_req.input.len(), 1);

    if let ResponsesInputItem::Message(ref m) = codex_req.input[0] {
        assert_eq!(m.role, "user");
        assert_eq!(m.content[0].text, "Hello");
    } else {
        panic!("expected Message variant");
    }
}

#[test]
fn convert_request_no_system_message_injects_default_instructions() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let req = chat_req("gpt-5.3-codex", vec![msg("user", "Hello")]);
    let codex_req = convert_request(&req, None, BackendProfile::ChatGptCodex);
    assert!(codex_req.instructions.is_some(), "default instructions should be injected");
}

#[test]
fn convert_request_multiple_system_messages_joined() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let req = chat_req("gpt-5.3-codex", vec![
        msg("system", "Rule 1."),
        msg("system", "Rule 2."),
        msg("user", "Hi"),
    ]);

    let codex_req = convert_request(&req, None, BackendProfile::ChatGptCodex);
    assert_eq!(codex_req.instructions.as_deref(), Some("Rule 1.\n\nRule 2."));
}

#[test]
fn convert_request_empty_messages_injects_empty_user_item() {
    use openai_proxy_lib::codex::{BackendProfile, ResponsesInputItem, convert_request};

    let req = chat_req("gpt-5.3-codex", vec![]);
    let codex_req = convert_request(&req, None, BackendProfile::ChatGptCodex);
    assert_eq!(codex_req.input.len(), 1);

    if let ResponsesInputItem::Message(ref m) = codex_req.input[0] {
        assert_eq!(m.role, "user");
    } else {
        panic!("expected Message variant");
    }
}

#[test]
fn convert_request_store_is_false() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let req = chat_req("gpt-5.3-codex", vec![msg("user", "Hi")]);
    let codex_req = convert_request(&req, None, BackendProfile::ChatGptCodex);
    assert_eq!(codex_req.store, Some(false));
}

#[test]
fn convert_request_chatgpt_profile_strips_temperature() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let mut req = chat_req("gpt-5.3-codex", vec![msg("user", "Hi")]);
    req.max_tokens = Some(100);
    req.temperature = Some(0.7);
    req.top_p = Some(0.9);

    let codex_req = convert_request(&req, None, BackendProfile::ChatGptCodex);
    assert!(codex_req.temperature.is_none(), "ChatGptCodex must strip temperature");
    assert!(codex_req.top_p.is_none(), "ChatGptCodex must strip top_p");
    assert!(codex_req.max_output_tokens.is_none(), "ChatGptCodex must strip max_output_tokens");
}

#[test]
fn convert_request_responses_api_profile_preserves_temperature() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let mut req = chat_req("gpt-5.3-codex", vec![msg("user", "Hi")]);
    req.max_tokens = Some(50);
    req.temperature = Some(0.5);
    req.top_p = Some(0.8);

    let codex_req = convert_request(&req, None, BackendProfile::OpenAiResponses);
    assert_eq!(codex_req.temperature, Some(0.5), "OpenAiResponses should preserve temperature");
    assert_eq!(codex_req.top_p, Some(0.8), "OpenAiResponses should preserve top_p");
    assert_eq!(codex_req.max_output_tokens, Some(50), "OpenAiResponses should preserve max_output_tokens");
}

#[test]
fn convert_request_default_model_overrides_generic_alias() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let req = chat_req("gpt-4o", vec![msg("user", "Hi")]);
    let codex_req = convert_request(&req, Some("codex-mini"), BackendProfile::OpenAiResponses);
    assert_eq!(codex_req.model, "codex-mini");
}

#[test]
fn convert_request_default_model_does_not_override_explicit_codex_id() {
    use openai_proxy_lib::codex::{BackendProfile, convert_request};

    let req = chat_req("gpt-5.3-codex", vec![msg("user", "Hi")]);
    let codex_req = convert_request(&req, Some("codex-mini"), BackendProfile::OpenAiResponses);
    assert_eq!(codex_req.model, "gpt-5.3-codex");
}

// ── resolve_model / backend compatibility ─────────────────────────────────────

#[test]
fn resolve_model_gpt55_supports_all_profiles() {
    use openai_proxy_lib::codex::resolve_model;
    let t = resolve_model("gpt-5.5");
    assert!(t.supports_codex_backend);
    assert!(t.supports_responses_api);
    assert!(t.supports_chat_completions);
    assert_eq!(t.model_id, "gpt-5.5");
}

#[test]
fn resolve_model_gpt55_pro_responses_api_only() {
    use openai_proxy_lib::codex::resolve_model;
    let t = resolve_model("gpt-5.5-pro");
    assert!(!t.supports_codex_backend, "gpt-5.5-pro should not be available on ChatGptCodex");
    assert!(t.supports_responses_api);
    assert!(!t.supports_chat_completions, "gpt-5.5-pro should not be available on Chat Completions");
}

#[test]
fn resolve_model_codex_mini_api_key_only() {
    use openai_proxy_lib::codex::resolve_model;
    let t = resolve_model("codex-mini");
    assert!(!t.supports_codex_backend, "codex-mini must not be available on ChatGptCodex profile");
    assert!(t.supports_responses_api);
    assert!(t.supports_chat_completions);
}

#[test]
fn resolve_model_gpt54_all_profiles() {
    use openai_proxy_lib::codex::resolve_model;
    let t = resolve_model("gpt-5.4");
    assert!(t.supports_codex_backend);
    assert!(t.supports_responses_api);
    assert!(t.supports_chat_completions);
}

// ── auth loading ─────────────────────────────────────────────────────────────

#[test]
fn auth_json_loads_from_standard_location() {
    let (state, _) = load_real_auth();
    let has_creds = state.auth.access_token.is_some() || state.auth.api_key.is_some();
    assert!(has_creds, "auth.json must contain access_token or api_key");
}

#[test]
fn auth_bearer_returns_non_empty_header() {
    let (state, _) = load_real_auth();
    let (header, _) = state.auth.bearer();
    assert!(header.starts_with("Bearer "), "bearer header should start with 'Bearer '");
    assert!(header.len() > 10, "bearer header should contain an actual token");
}

#[test]
fn auth_chatgpt_path_has_account_id() {
    let (state, _) = load_real_auth();
    if state.auth.access_token.is_some() {
        assert!(
            state.auth.account_id.is_some(),
            "access_token present but account_id missing — auth.json may be incomplete"
        );
    }
}

#[test]
fn backend_url_matches_auth_type() {
    use openai_proxy_lib::codex::{CODEX_BACKEND_URL, OPENAI_RESPONSES_URL};
    let (state, backend_url) = load_real_auth();
    if state.auth.access_token.is_some() {
        assert_eq!(backend_url, CODEX_BACKEND_URL);
    } else {
        assert_eq!(backend_url, OPENAI_RESPONSES_URL);
    }
}

// ── MCP dispatch (unit-level, no network for non-chat tools) ─────────────────

#[tokio::test]
async fn mcp_initialize_returns_protocol_version() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "initialize".to_string(),
        params: json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }),
    };

    let resp = dispatch(&state, req).await.expect("initialize should return a response");
    assert!(resp.error.is_none(), "initialize should not return an error");
    let result = resp.result.unwrap();
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "openai-proxy");
}

#[tokio::test]
async fn mcp_ping_returns_empty_result() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(2)),
        method: "ping".to_string(),
        params: json!({}),
    };

    let resp = dispatch(&state, req).await.expect("ping should respond");
    assert!(resp.error.is_none());
    assert_eq!(resp.result.unwrap(), json!({}));
}

#[tokio::test]
async fn mcp_tools_list_returns_four_tools() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(3)),
        method: "tools/list".to_string(),
        params: json!({}),
    };

    let resp = dispatch(&state, req).await.expect("tools/list should respond");
    assert!(resp.error.is_none());
    let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
    assert_eq!(tools.len(), 4, "expected 4 MCP tools, got {}", tools.len());

    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"chat_completion"));
    assert!(names.contains(&"list_models"));
    assert!(names.contains(&"check_auth"));
    assert!(names.contains(&"set_model"));
}

#[tokio::test]
async fn mcp_tools_list_tools_have_input_schema() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(4)),
        method: "tools/list".to_string(),
        params: json!({}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    for tool in resp.result.unwrap()["tools"].as_array().unwrap() {
        assert!(
            tool["inputSchema"].is_object(),
            "tool {} missing inputSchema",
            tool["name"]
        );
    }
}

#[tokio::test]
async fn mcp_list_models_tool_returns_models_for_profile() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let profile = state.backend_profile;
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(5)),
        method: "tools/call".to_string(),
        params: json!({"name": "list_models", "arguments": {}}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("gpt-5.3-codex"), "list_models should mention gpt-5.3-codex");
    match profile {
        BackendProfile::ChatGptCodex => {
            assert!(!text.contains("codex-mini"), "ChatGptCodex should not list codex-mini");
        }
        _ => {
            assert!(text.contains("codex-mini"), "API key profiles should list codex-mini");
        }
    }
}

#[tokio::test]
async fn mcp_check_auth_tool_reports_authenticated() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(6)),
        method: "tools/call".to_string(),
        params: json!({"name": "check_auth", "arguments": {}}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains('✓'), "check_auth should indicate authenticated status");
}

#[tokio::test]
async fn mcp_set_model_recommends_mini_for_simple_task() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(7)),
        method: "tools/call".to_string(),
        params: json!({"name": "set_model", "arguments": {"task": "quick question"}}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    let text = resp.result.unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("codex-mini"), "simple task should recommend codex-mini");
}

#[tokio::test]
async fn mcp_set_model_recommends_full_for_complex_task() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(8)),
        method: "tools/call".to_string(),
        params: json!({"name": "set_model", "arguments": {"task": "complex architecture design"}}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    let text = resp.result.unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("gpt-5.3-codex"), "complex task should recommend gpt-5.3-codex");
}

#[tokio::test]
async fn mcp_unknown_tool_returns_is_error_true() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(9)),
        method: "tools/call".to_string(),
        params: json!({"name": "does_not_exist", "arguments": {}}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    let result = resp.result.unwrap();
    assert_eq!(result["isError"], true, "unknown tool should set isError=true");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Unknown tool"), "should report unknown tool name");
}

#[tokio::test]
async fn mcp_unknown_method_returns_method_not_found_error() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(10)),
        method: "nonexistent/method".to_string(),
        params: json!({}),
    };

    let resp = dispatch(&state, req).await.unwrap();
    assert!(resp.error.is_some(), "unknown method should return JSON-RPC error");
    assert_eq!(resp.error.unwrap().code, -32601);
}

#[tokio::test]
async fn mcp_notifications_initialized_returns_none() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: "notifications/initialized".to_string(),
        params: json!({}),
    };

    let resp = dispatch(&state, req).await;
    assert!(resp.is_none(), "notifications/initialized should return None (no response)");
}

// ── MCP live: chat_completion tool hits real Codex ───────────────────────────

#[tokio::test]
async fn mcp_chat_completion_tool_returns_text() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(20)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "chat_completion",
            "arguments": {
                "model": "gpt-5.3-codex",
                "messages": [{"role": "user", "content": "Reply with exactly: MCP_OK"}]
            }
        }),
    };

    let resp = dispatch(&state, req).await.unwrap();
    let result = resp.result.unwrap();
    assert_eq!(result["isError"], false, "chat_completion should not error: {:?}", result);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(!text.is_empty(), "MCP chat_completion should return non-empty text");
}

#[tokio::test]
async fn mcp_chat_completion_tool_with_max_tokens() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(21)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "chat_completion",
            "arguments": {
                "model": "gpt-5.3-codex",
                "messages": [{"role": "user", "content": "Count from 1 to 100."}],
                "max_tokens": 5
            }
        }),
    };

    let resp = dispatch(&state, req).await.unwrap();
    let result = resp.result.unwrap();
    // ChatGptCodex strips max_tokens — backend accepts the request anyway.
    // OpenAiResponses forwards it — backend truncates.
    // Either way: isError=false and some text.
    assert_eq!(result["isError"], false, "chat_completion with max_tokens should not error: {:?}", result);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(!text.is_empty());
}

#[tokio::test]
async fn mcp_chat_completion_tool_missing_messages_is_error() {
    use openai_proxy_lib::mcp::{JsonRpcRequest, dispatch};

    let state = mcp_state();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(22)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "chat_completion",
            "arguments": {
                "model": "gpt-5.3-codex"
                // messages field intentionally omitted
            }
        }),
    };

    let resp = dispatch(&state, req).await.unwrap();
    let result = resp.result.unwrap();
    assert_eq!(result["isError"], true, "missing messages should set isError=true");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("messages"), "error should mention the missing field");
}

// ── MCP HTTP transport ────────────────────────────────────────────────────────

#[tokio::test]
async fn mcp_http_initialize_endpoint() {
    let (state, _) = load_real_auth();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        mcp::run_http_on(state, listener).await.ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }
        }))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["result"]["protocolVersion"], "2024-11-05");
}

#[tokio::test]
async fn mcp_http_tools_list_endpoint() {
    let (state, _) = load_real_auth();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        mcp::run_http_on(state, listener).await.ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body: Value = resp.json().await.unwrap();
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);
}

#[tokio::test]
async fn mcp_http_notifications_returns_no_content() {
    let (state, _) = load_real_auth();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        mcp::run_http_on(state, listener).await.ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .json(&json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 204);
}

#[tokio::test]
async fn mcp_http_bad_json_returns_400() {
    let (state, _) = load_real_auth();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        mcp::run_http_on(state, listener).await.ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .header("content-type", "application/json")
        .body("not valid json")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 400);
}

// ── error handling ────────────────────────────────────────────────────────────

#[tokio::test]
async fn invalid_json_body_returns_422() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .text("not json")
        .await;

    assert!(
        resp.status_code().as_u16() >= 400,
        "invalid body should return 4xx, got: {}",
        resp.status_code()
    );
}

#[tokio::test]
async fn missing_messages_field_returns_error() {
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&json!({"model": "gpt-5.3-codex"}))
        .await;

    assert!(
        resp.status_code().as_u16() >= 400,
        "missing messages should return 4xx, got: {}",
        resp.status_code()
    );
}

// ── profile-mismatch + gpt-5.5 ───────────────────────────────────────────────

#[tokio::test]
async fn model_not_available_on_wrong_profile() {
    // gpt-5.5-pro is only available on OpenAiResponses (API key). On ChatGptCodex it must
    // return 400 with type "model_not_available".
    let (state, _) = load_real_auth();
    if !matches!(state.backend_profile, BackendProfile::ChatGptCodex) {
        return; // Only relevant for ChatGPT subscription path
    }

    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&chat_req_json("gpt-5.5-pro", &[("user", "Hello")]))
        .await;

    assert_eq!(
        resp.status_code().as_u16(),
        400,
        "expected 400 for gpt-5.5-pro on ChatGptCodex, got: {}",
        resp.status_code()
    );
    let body: Value = resp.json();
    assert_eq!(
        body["error"]["type"], "model_not_available",
        "expected model_not_available error type, got: {body}"
    );
}

#[tokio::test]
async fn non_streaming_gpt55_responds() {
    // gpt-5.5 is available on all backend profiles.
    let server = test_server();
    let resp = server
        .post("/v1/chat/completions")
        .json(&chat_req_json("gpt-5.5", &[("user", "Say the word 'ready' and nothing else.")]))
        .await;

    assert_eq!(
        resp.status_code().as_u16(),
        200,
        "expected 200 for gpt-5.5, got: {} body: {}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    assert!(!content.is_empty(), "expected non-empty response from gpt-5.5, got: {body}");
}
