use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response, Sse, sse::Event},
};
use futures_util::StreamExt;
use uuid::Uuid;

use crate::{
    AppState,
    codex::{
        self, BackendProfile, ChatCompletionChunk, ChunkChoice, Delta, PendingFunctionCall,
        ResponseStreamEvent, ToolCallDelta, ToolCallFunctionDelta,
        build_chat_completions_request, resolve_model,
    },
    error::ProxyError,
    hooks::HookEvent,
    openai::{
        ChatCompletionRequest, ChatCompletionResponse, Choice, Message, MessageContent,
        ResponseMessage, ToolCall, ToolCallFunction, Usage,
    },
    skills::select_skills,
};

pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, ProxyError> {
    // Hook: RequestReceived
    state
        .hooks
        .fire(HookEvent::RequestReceived {
            model: req.model.clone(),
            message_count: req.messages.len(),
        })
        .await;

    // Inject relevant skills as a system message prefix when skills are loaded.
    let req = inject_skills(req, &state);

    // Inject memory context (RAG) when memory feature is enabled.
    let memory_scope = headers
        .get("x-memory-scope")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("session")
        .to_string();
    let req = inject_memory(req, &state, &memory_scope).await;

    let target = resolve_model(&req.model);
    let available = match state.backend_profile {
        BackendProfile::ChatGptCodex => target.supports_codex_backend,
        BackendProfile::OpenAiResponses => target.supports_responses_api,
        BackendProfile::OpenAiChatCompletions => target.supports_chat_completions,
    };
    if !available {
        let err = ProxyError::ModelNotAvailable {
            model: target.model_id,
            profile: state.backend_profile.name().to_string(),
        };
        state
            .hooks
            .fire(HookEvent::Error {
                status: 400,
                message: err.to_string(),
            })
            .await;
        return Err(err);
    }

    match state.backend_profile {
        BackendProfile::OpenAiChatCompletions => {
            if req.stream {
                stream_chat_completions(state, req).await
            } else {
                non_stream_chat_completions(state, req).await
            }
        }
        _ => {
            if req.stream {
                stream_responses(state, req).await
            } else {
                non_stream_responses(state, req).await
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Responses API — non-streaming (consumes SSE internally)
// ---------------------------------------------------------------------------

async fn non_stream_responses(
    state: AppState,
    req: ChatCompletionRequest,
) -> Result<Response, ProxyError> {
    let model_id = req.model.clone();
    let codex_req =
        codex::convert_request(&req, state.default_model.as_deref(), state.backend_profile);

    tracing::debug!(
        model = %codex_req.model,
        items = codex_req.input.len(),
        "forwarding non-streaming request (consuming SSE)"
    );

    let http_req = build_responses_request(&state, &codex_req)?;
    let resp = http_req.send().await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        state
            .hooks
            .fire(HookEvent::Error {
                status: status.as_u16(),
                message: body.clone(),
            })
            .await;
        return Err(ProxyError::Upstream { status: status.as_u16(), body });
    }

    let raw = resp.text().await?;
    let mut response_id = String::new();
    let mut content_parts: Vec<String> = Vec::new();
    let mut finish_reason = "stop".to_string();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;

    // Tool call accumulators keyed by output_index.
    let mut pending_calls: HashMap<u32, PendingFunctionCall> = HashMap::new();
    // Ordered list of output_indices that correspond to function calls (preserves order).
    let mut call_order: Vec<u32> = Vec::new();

    for line in raw.lines() {
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data == "[DONE]" {
            break;
        }
        let Ok(event) = serde_json::from_str::<codex::ResponseStreamEvent>(data) else {
            continue;
        };
        match event {
            codex::ResponseStreamEvent::ResponseCreated { response } => {
                response_id = response.id;
            }
            codex::ResponseStreamEvent::ResponseOutputItemAdded { output_index, item } => {
                if item.item_type.as_deref() == Some("function_call") {
                    let call_id = item.call_id.clone().unwrap_or_default();
                    let name = item.name.clone().unwrap_or_default();
                    // Hook: ToolCallStart
                    state
                        .hooks
                        .fire(HookEvent::ToolCallStart {
                            name: name.clone(),
                            call_id: call_id.clone(),
                        })
                        .await;
                    pending_calls.insert(
                        output_index,
                        PendingFunctionCall {
                            output_index,
                            call_id,
                            name,
                            arguments: String::new(),
                        },
                    );
                    call_order.push(output_index);
                }
            }
            codex::ResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                output_index,
                delta,
            } => {
                if let Some(call) = pending_calls.get_mut(&output_index) {
                    // Hook: ToolCallArgs
                    state
                        .hooks
                        .fire(HookEvent::ToolCallArgs {
                            call_id: call.call_id.clone(),
                            args_delta: delta.clone(),
                        })
                        .await;
                    call.arguments.push_str(&delta);
                }
            }
            codex::ResponseStreamEvent::ResponseFunctionCallArgumentsDone {
                output_index,
                arguments,
            } => {
                if let Some(call) = pending_calls.get_mut(&output_index) {
                    call.arguments = arguments;
                }
            }
            codex::ResponseStreamEvent::ResponseOutputItemDone { output_index, item } => {
                if item.item_type.as_deref() == Some("function_call") {
                    if let Some(call) = pending_calls.get_mut(&output_index) {
                        // Prefer the final authoritative fields from the done event.
                        if let Some(cid) = item.call_id {
                            call.call_id = cid;
                        }
                        if let Some(n) = item.name {
                            call.name = n;
                        }
                        if let Some(args) = item.arguments {
                            call.arguments = args;
                        }
                    }
                }
            }
            codex::ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } => {
                // Hook: TextDelta
                state
                    .hooks
                    .fire(HookEvent::TextDelta { delta: delta.clone() })
                    .await;
                content_parts.push(delta);
            }
            codex::ResponseStreamEvent::ResponseDone { response } => {
                if response_id.is_empty() {
                    response_id = response.id;
                }
                finish_reason = codex::map_finish_reason(response.status.as_deref());
                // Hook: ResponseComplete
                state
                    .hooks
                    .fire(HookEvent::ResponseComplete { finish_reason: finish_reason.clone() })
                    .await;
                if let Some(usage) = response.usage {
                    input_tokens = usage.input_tokens;
                    output_tokens = usage.output_tokens;
                }
            }
            _ => {}
        }
    }

    // Check if the request had tool results — fire ToolResultSubmitted for each tool message.
    for msg in &req.messages {
        if msg.role == "tool" {
            let call_id = msg
                .tool_call_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            state
                .hooks
                .fire(HookEvent::ToolResultSubmitted { call_id })
                .await;
        }
    }

    // If any function calls were accumulated, build the response with tool_calls.
    let (response_content, tool_calls) = if !call_order.is_empty() {
        let calls: Vec<ToolCall> = call_order
            .iter()
            .filter_map(|idx| pending_calls.remove(idx))
            .map(PendingFunctionCall::into_tool_call)
            .collect();
        // finish_reason should be "tool_calls" if we got calls but the event didn't say so.
        if finish_reason == "stop" {
            finish_reason = "tool_calls".to_string();
        }
        let text = content_parts.join("");
        (if text.is_empty() { None } else { Some(text) }, Some(calls))
    } else {
        let text = content_parts.join("");
        (if text.is_empty() { None } else { Some(text) }, None)
    };

    let openai_resp = ChatCompletionResponse {
        id: format!("chatcmpl-{response_id}"),
        object: "chat.completion",
        created: unix_now(),
        model: model_id,
        choices: vec![Choice {
            index: 0,
            message: ResponseMessage {
                role: "assistant",
                content: response_content,
                tool_calls,
            },
            finish_reason,
        }],
        usage: Usage {
            prompt_tokens: input_tokens,
            completion_tokens: output_tokens,
            total_tokens: input_tokens + output_tokens,
        },
    };

    Ok(Json(openai_resp).into_response())
}

// ---------------------------------------------------------------------------
// Responses API — streaming
// ---------------------------------------------------------------------------

async fn stream_responses(
    state: AppState,
    req: ChatCompletionRequest,
) -> Result<Response, ProxyError> {
    let model_id = req.model.clone();
    let codex_req =
        codex::convert_request(&req, state.default_model.as_deref(), state.backend_profile);
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());

    tracing::debug!(
        model = %codex_req.model,
        items = codex_req.input.len(),
        "forwarding streaming request to Responses API"
    );

    // Fire ToolResultSubmitted for any tool messages in the incoming request.
    for msg in &req.messages {
        if msg.role == "tool" {
            let call_id = msg
                .tool_call_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            state
                .hooks
                .fire(HookEvent::ToolResultSubmitted { call_id })
                .await;
        }
    }

    let http_req = build_responses_request(&state, &codex_req)?;
    let resp = http_req.send().await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        state
            .hooks
            .fire(HookEvent::Error {
                status: status.as_u16(),
                message: body.clone(),
            })
            .await;
        return Err(ProxyError::Upstream { status: status.as_u16(), body });
    }

    let byte_stream = resp.bytes_stream();
    let created = unix_now();
    let id = completion_id.clone();
    let model = model_id.clone();
    // Clone the hooks Arc so it can be moved into the stream closure.
    let hooks = state.hooks.clone();

    let line_stream = byte_stream
        .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())))
        .scan(String::new(), |buf, chunk_result| {
            let lines = match chunk_result {
                Ok(bytes) => {
                    buf.push_str(&String::from_utf8_lossy(&bytes));
                    let mut lines = Vec::new();
                    while let Some(pos) = buf.find('\n') {
                        let line = buf.drain(..=pos).collect::<String>();
                        lines.push(line.trim_end().to_string());
                    }
                    lines
                }
                Err(e) => {
                    tracing::error!(error = %e, "stream read error");
                    vec![]
                }
            };
            std::future::ready(Some(futures_util::stream::iter(lines)))
        })
        .flatten();

    let sse_stream = line_stream.filter_map({
            let id = id.clone();
            let model = model.clone();
            // We need mutable state for call_index_map inside filter_map.
            // Use a Mutex-wrapped map to share across the async closure.
            let call_index_map = std::sync::Arc::new(std::sync::Mutex::new(
                HashMap::<u32, u32>::new(),
            ));
            // Pending call_ids for streaming hook purposes (call_id by output_index).
            let pending_call_ids = std::sync::Arc::new(std::sync::Mutex::new(
                HashMap::<u32, String>::new(),
            ));
            move |line| {
                let id = id.clone();
                let model = model.clone();
                let call_index_map = call_index_map.clone();
                let pending_call_ids = pending_call_ids.clone();
                let hooks = hooks.clone();
                async move {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return None;
                        }
                        match serde_json::from_str::<ResponseStreamEvent>(data) {
                            Ok(event) => {
                                // Fire hooks based on the event type before building SSE.
                                match &event {
                                    ResponseStreamEvent::ResponseOutputItemAdded { output_index, item } => {
                                        if item.item_type.as_deref() == Some("function_call") {
                                            let call_id = item.call_id.clone().unwrap_or_default();
                                            let name = item.name.clone().unwrap_or_default();
                                            pending_call_ids.lock().unwrap().insert(*output_index, call_id.clone());
                                            hooks.fire(HookEvent::ToolCallStart { name, call_id }).await;
                                        }
                                    }
                                    ResponseStreamEvent::ResponseFunctionCallArgumentsDelta { output_index, delta } => {
                                        let call_id = pending_call_ids
                                            .lock()
                                            .unwrap()
                                            .get(output_index)
                                            .cloned()
                                            .unwrap_or_default();
                                        hooks.fire(HookEvent::ToolCallArgs { call_id, args_delta: delta.clone() }).await;
                                    }
                                    ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } => {
                                        hooks.fire(HookEvent::TextDelta { delta: delta.clone() }).await;
                                    }
                                    ResponseStreamEvent::ResponseDone { response } => {
                                        let finish = codex::map_finish_reason(response.status.as_deref());
                                        hooks.fire(HookEvent::ResponseComplete { finish_reason: finish }).await;
                                    }
                                    _ => {}
                                }
                                build_sse_event(&id, created, &model, event, &call_index_map)
                            }
                            Err(e) => {
                                tracing::trace!(
                                    error = %e,
                                    data = data,
                                    "unrecognized Responses SSE event"
                                );
                                None
                            }
                        }
                    } else {
                        None
                    }
                }
            }
        });

    Ok(Sse::new(sse_stream).into_response())
}

// ---------------------------------------------------------------------------
// Chat Completions — non-streaming
// ---------------------------------------------------------------------------

async fn non_stream_chat_completions(
    state: AppState,
    req: ChatCompletionRequest,
) -> Result<Response, ProxyError> {
    let model_id = req.model.clone();
    let outbound = build_chat_completions_request(&req, state.default_model.as_deref());

    tracing::debug!(
        model = %outbound.model,
        "forwarding non-streaming request to Chat Completions API"
    );

    // Fire ToolResultSubmitted for tool messages in the incoming request.
    for msg in &req.messages {
        if msg.role == "tool" {
            let call_id = msg
                .tool_call_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            state
                .hooks
                .fire(HookEvent::ToolResultSubmitted { call_id })
                .await;
        }
    }

    let (auth_header, _) = state.auth.bearer();
    let resp = state
        .http_client
        .post(&state.backend_url)
        .header("authorization", &auth_header)
        .header("content-type", "application/json")
        .json(&outbound)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        state
            .hooks
            .fire(HookEvent::Error {
                status: status.as_u16(),
                message: body.clone(),
            })
            .await;
        return Err(ProxyError::Upstream { status: status.as_u16(), body });
    }

    let body: serde_json::Value = resp.json().await?;

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string());
    let finish_reason = body["choices"][0]["finish_reason"]
        .as_str()
        .unwrap_or("stop")
        .to_string();
    let response_id = body["id"].as_str().unwrap_or("").to_string();
    let prompt_tokens = body["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
    let completion_tokens = body["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;

    // Hook: ResponseComplete
    state
        .hooks
        .fire(HookEvent::ResponseComplete { finish_reason: finish_reason.clone() })
        .await;

    // Extract tool_calls from the Chat Completions response if present.
    let tool_calls = parse_chat_completions_tool_calls(&body["choices"][0]["message"]["tool_calls"]);

    let openai_resp = ChatCompletionResponse {
        id: response_id,
        object: "chat.completion",
        created: unix_now(),
        model: model_id,
        choices: vec![Choice {
            index: 0,
            message: ResponseMessage { role: "assistant", content, tool_calls },
            finish_reason,
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    };

    Ok(Json(openai_resp).into_response())
}

// ---------------------------------------------------------------------------
// Chat Completions — streaming
// ---------------------------------------------------------------------------

async fn stream_chat_completions(
    state: AppState,
    req: ChatCompletionRequest,
) -> Result<Response, ProxyError> {
    let outbound = build_chat_completions_request(&req, state.default_model.as_deref());

    tracing::debug!(
        model = %outbound.model,
        "forwarding streaming request to Chat Completions API"
    );

    // Fire ToolResultSubmitted for tool messages in the incoming request.
    for msg in &req.messages {
        if msg.role == "tool" {
            let call_id = msg
                .tool_call_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            state
                .hooks
                .fire(HookEvent::ToolResultSubmitted { call_id })
                .await;
        }
    }

    let (auth_header, _) = state.auth.bearer();
    let resp = state
        .http_client
        .post(&state.backend_url)
        .header("authorization", &auth_header)
        .header("content-type", "application/json")
        .json(&outbound)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        state
            .hooks
            .fire(HookEvent::Error {
                status: status.as_u16(),
                message: body.clone(),
            })
            .await;
        return Err(ProxyError::Upstream { status: status.as_u16(), body });
    }

    // Chat Completions streams standard SSE with OpenAI chunk format — pass through verbatim.
    // tool_calls deltas are already in the correct format and do not need translation.
    let byte_stream = resp.bytes_stream();
    let stream = byte_stream.map(|r| {
        r.map(|b| Event::default().data(String::from_utf8_lossy(&b).into_owned()))
            .map_err(|e| std::io::Error::other(e.to_string()))
    });

    Ok(Sse::new(stream).into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn build_responses_request(
    state: &AppState,
    codex_req: &codex::ResponsesRequest,
) -> Result<reqwest::RequestBuilder, ProxyError> {
    let (auth_header, account_id) = state.auth.bearer();

    let mut req_builder = state
        .http_client
        .post(&state.backend_url)
        .header("authorization", &auth_header)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .header("accept-language", "en-US,en;q=0.9")
        .header("origin", "https://chatgpt.com")
        .header("referer", "https://chatgpt.com/")
        .header("sec-fetch-dest", "empty")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header("openai-beta", "responses=experimental")
        .header("originator", "codex_cli_rs")
        .json(codex_req);

    if let Some(id) = account_id {
        req_builder = req_builder.header("chatgpt-account-id", id);
    }

    Ok(req_builder)
}

/// Parse tool_calls from a Chat Completions response message JSON value.
fn parse_chat_completions_tool_calls(value: &serde_json::Value) -> Option<Vec<ToolCall>> {
    let arr = value.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let calls: Vec<ToolCall> = arr
        .iter()
        .filter_map(|tc| {
            let id = tc["id"].as_str()?.to_string();
            let call_type = tc["type"].as_str().unwrap_or("function").to_string();
            let name = tc["function"]["name"].as_str()?.to_string();
            let arguments = tc["function"]["arguments"].as_str().unwrap_or("{}").to_string();
            Some(ToolCall {
                id,
                call_type,
                function: ToolCallFunction { name, arguments },
            })
        })
        .collect();
    if calls.is_empty() { None } else { Some(calls) }
}

fn build_sse_event(
    id: &str,
    created: u64,
    model: &str,
    event: ResponseStreamEvent,
    call_index_map: &std::sync::Arc<std::sync::Mutex<HashMap<u32, u32>>>,
) -> Option<Result<Event, std::convert::Infallible>> {
    let chunk: Option<ChatCompletionChunk> = match event {
        ResponseStreamEvent::ResponseCreated { .. } => Some(ChatCompletionChunk {
            id: id.to_string(),
            object: "chat.completion.chunk",
            created,
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta { role: Some("assistant"), content: None, tool_calls: None },
                finish_reason: None,
            }],
        }),

        ResponseStreamEvent::ResponseOutputItemAdded { output_index, item } => {
            if item.item_type.as_deref() == Some("function_call") {
                // Assign a sequential tool_calls array index to this output_index.
                let tc_index = {
                    let mut map = call_index_map.lock().unwrap();
                    let next = map.len() as u32;
                    map.insert(output_index, next);
                    next
                };
                let call_id = item.call_id.clone().unwrap_or_default();
                let name = item.name.clone().unwrap_or_default();
                // Emit first chunk: role + tool_calls header (id, type, name, empty args).
                Some(ChatCompletionChunk {
                    id: id.to_string(),
                    object: "chat.completion.chunk",
                    created,
                    model: model.to_string(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: None,
                            tool_calls: Some(vec![ToolCallDelta {
                                index: tc_index,
                                id: Some(call_id),
                                call_type: Some("function"),
                                function: ToolCallFunctionDelta {
                                    name: Some(name),
                                    arguments: String::new(),
                                },
                            }]),
                        },
                        finish_reason: None,
                    }],
                })
            } else {
                None
            }
        }

        ResponseStreamEvent::ResponseFunctionCallArgumentsDelta { output_index, delta } => {
            let tc_index = {
                let map = call_index_map.lock().unwrap();
                *map.get(&output_index)?
            };
            Some(ChatCompletionChunk {
                id: id.to_string(),
                object: "chat.completion.chunk",
                created,
                model: model.to_string(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: None,
                        tool_calls: Some(vec![ToolCallDelta {
                            index: tc_index,
                            id: None,
                            call_type: None,
                            function: ToolCallFunctionDelta {
                                name: None,
                                arguments: delta,
                            },
                        }]),
                    },
                    finish_reason: None,
                }],
            })
        }

        ResponseStreamEvent::ResponseOutputItemDone { output_index, item } => {
            // For function_call items, emit a finish_reason="tool_calls" chunk.
            if item.item_type.as_deref() == Some("function_call") {
                let tc_index = {
                    let map = call_index_map.lock().unwrap();
                    *map.get(&output_index)?
                };
                // If this is the last (or only) tool call, emit the finish chunk.
                // We emit a zero-content delta with finish_reason="tool_calls".
                let _ = tc_index; // used above for index lookup; chunk emitted below
                let finish_chunk = ChatCompletionChunk {
                    id: id.to_string(),
                    object: "chat.completion.chunk",
                    created,
                    model: model.to_string(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: Delta { role: None, content: None, tool_calls: None },
                        finish_reason: Some("tool_calls".to_string()),
                    }],
                };
                let json = serde_json::to_string(&finish_chunk).ok()?;
                return Some(Ok(Event::default().data(json)));
            }
            None
        }

        ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } => Some(ChatCompletionChunk {
            id: id.to_string(),
            object: "chat.completion.chunk",
            created,
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta { role: None, content: Some(delta), tool_calls: None },
                finish_reason: None,
            }],
        }),

        ResponseStreamEvent::ResponseDone { response } => {
            let finish = codex::map_finish_reason(response.status.as_deref());
            // Only emit text-path finish chunk if no function calls were in progress.
            // (Function calls emit their own finish chunk via ResponseOutputItemDone.)
            let has_calls = {
                let map = call_index_map.lock().unwrap();
                !map.is_empty()
            };
            if has_calls {
                return None;
            }
            let finish_chunk = ChatCompletionChunk {
                id: id.to_string(),
                object: "chat.completion.chunk",
                created,
                model: model.to_string(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: None, tool_calls: None },
                    finish_reason: Some(finish),
                }],
            };
            let json = serde_json::to_string(&finish_chunk).ok()?;
            return Some(Ok(Event::default().data(json)));
        }

        _ => None,
    };

    let chunk = chunk?;
    let json = serde_json::to_string(&chunk).ok()?;
    Some(Ok(Event::default().data(json)))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// If skills are loaded, select relevant ones and prepend their content as a
/// system message. Returns the (possibly modified) request.
fn inject_skills(mut req: ChatCompletionRequest, state: &AppState) -> ChatCompletionRequest {
    // Inject skill context as a system message.
    if !state.skills.is_empty() {
        let max = std::env::var("PROXY_SKILLS_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(3);

        let user_text: String = req
            .messages
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| match &m.content {
                crate::openai::MessageContent::Text(t) => t.as_str(),
                _ => "",
            })
            .collect::<Vec<_>>()
            .join(" ");

        let selected = select_skills(&user_text, &state.skills, max, state.backend_profile.name());
        if !selected.is_empty() {
            let skill_block = selected
                .iter()
                .map(|s| s.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");

            let prefix_msg = Message {
                role: "system".to_string(),
                content: MessageContent::Text(format!("# Active Skills\n\n{skill_block}")),
                tool_call_id: None,
                tool_calls: None,
                name: None,
            };
            req.messages.insert(0, prefix_msg);
        }
    }

    // Inject MCP tool schemas when configured and request doesn't already include them.
    if !state.mcp_tools.is_empty() {
        // Extract existing tool names from raw JSON tools array.
        let existing_names: std::collections::HashSet<String> = req
            .tools
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("function")?.get("name")?.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        let new_tools: Vec<serde_json::Value> = state
            .mcp_tools
            .iter()
            .filter(|mcp| !existing_names.contains(&mcp.function.name))
            .filter_map(|mcp| serde_json::to_value(mcp).ok())
            .collect();

        if !new_tools.is_empty() {
            let tools_arr = req.tools.get_or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let Some(arr) = tools_arr.as_array_mut() {
                arr.extend(new_tools);
            }
        }
    }

    req
}

/// Inject relevant memory documents as a system context message (RAG injection).
/// Only active when the `memory` feature is compiled in and a memory store is present.
async fn inject_memory(
    #[cfg_attr(not(feature = "memory"), allow(unused_mut))]
    mut req: ChatCompletionRequest,
    #[cfg_attr(not(feature = "memory"), allow(unused_variables))]
    state: &AppState,
    #[cfg_attr(not(feature = "memory"), allow(unused_variables))]
    scope: &str,
) -> ChatCompletionRequest {
    #[cfg(feature = "memory")]
    {
        let Some(ref store) = state.memory_store else { return req };
        if !store.is_enabled() { return req; }

        let user_text: String = req.messages.iter()
            .filter(|m| m.role == "user")
            .map(|m| match &m.content {
                MessageContent::Text(t) => t.as_str(),
                _ => "",
            })
            .collect::<Vec<_>>()
            .join(" ");

        if user_text.is_empty() { return req; }

        let results = match tokio::time::timeout(
            std::time::Duration::from_millis(500),
            store.search(&user_text, scope, 3),
        ).await {
            Ok(Ok(docs)) if !docs.is_empty() => docs,
            _ => return req,
        };

        let context_block = results.iter()
            .map(|d| format!("- {}", d.text))
            .collect::<Vec<_>>()
            .join("\n");

        let context_msg = Message {
            role: "system".to_string(),
            content: MessageContent::Text(format!("# Relevant Context\n\n{context_block}")),
            tool_call_id: None,
            tool_calls: None,
            name: None,
        };

        req.messages.insert(0, context_msg);
    }
    req
}
