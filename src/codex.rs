use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::{ChatCompletionRequest, MessageContent, ToolCall, ToolCallFunction};

// ---------------------------------------------------------------------------
// auth.json deserialization
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct CodexAuth {
    pub access_token: Option<String>,
    pub account_id: Option<String>,
    pub api_key: Option<String>,
}

/// Nested format used by Codex CLI ≥ v1.x: `{ "tokens": { "access_token": ..., "account_id": ... } }`
#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    access_token: Option<String>,
    account_id: Option<String>,
    api_key: Option<String>,
    tokens: Option<TokensBlock>,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokensBlock {
    access_token: Option<String>,
    account_id: Option<String>,
}

impl CodexAuth {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let raw: CodexAuthFile = serde_json::from_str(&content)?;
        let (access_token, account_id) = if let Some(t) = raw.tokens {
            (t.access_token, t.account_id)
        } else {
            (raw.access_token, raw.account_id)
        };
        let api_key = raw.api_key.or(raw.openai_api_key);
        Ok(Self { access_token, account_id, api_key })
    }

    pub fn bearer(&self) -> (String, Option<String>) {
        if let (Some(token), Some(account_id)) = (&self.access_token, &self.account_id) {
            (format!("Bearer {token}"), Some(account_id.clone()))
        } else if let Some(key) = &self.api_key {
            (format!("Bearer {key}"), None)
        } else {
            panic!("auth.json contains neither access_token nor api_key");
        }
    }
}

// ---------------------------------------------------------------------------
// Backend profile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendProfile {
    /// `chatgpt.com/backend-api/codex/responses` — ChatGPT OAuth subscription.
    /// Requires stream=true, store=false. Rejects temperature, top_p, max_output_tokens.
    /// Models: gpt-5.3-codex, gpt-5.4, gpt-5.5
    ChatGptCodex,

    /// `api.openai.com/v1/responses` — OpenAI API key, Responses API wire format.
    /// Supports max_output_tokens, temperature (non-reasoning models), tools.
    /// Models: gpt-5.5, gpt-5.5-pro, gpt-5.4, gpt-5.3-codex, codex-mini
    OpenAiResponses,

    /// `api.openai.com/v1/chat/completions` — OpenAI API key, Chat Completions wire format.
    /// Uses messages[] array, max_completion_tokens.
    /// Opt-in via CODEX_WIRE_API=chat.
    OpenAiChatCompletions,
}

impl BackendProfile {
    pub fn backend_url(&self) -> &'static str {
        match self {
            BackendProfile::ChatGptCodex => CODEX_BACKEND_URL,
            BackendProfile::OpenAiResponses => OPENAI_RESPONSES_URL,
            BackendProfile::OpenAiChatCompletions => OPENAI_CHAT_URL,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BackendProfile::ChatGptCodex => "ChatGPT subscription (chatgpt.com/backend-api/codex/responses)",
            BackendProfile::OpenAiResponses => "OpenAI Responses API (api.openai.com/v1/responses)",
            BackendProfile::OpenAiChatCompletions => "OpenAI Chat Completions (api.openai.com/v1/chat/completions)",
        }
    }
}

// ---------------------------------------------------------------------------
// Backend URL constants
// ---------------------------------------------------------------------------

pub const CODEX_BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
pub const OPENAI_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";

// ---------------------------------------------------------------------------
// Model catalogue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModelTarget {
    pub model_id: String,
    pub supports_codex_backend: bool,
    pub supports_responses_api: bool,
    pub supports_chat_completions: bool,
}

pub fn resolve_model(input: &str) -> ModelTarget {
    match input {
        "gpt-5.5" => ModelTarget {
            model_id: "gpt-5.5".into(),
            supports_codex_backend: true,
            supports_responses_api: true,
            supports_chat_completions: true,
        },
        "gpt-5.5-pro" => ModelTarget {
            model_id: "gpt-5.5-pro".into(),
            supports_codex_backend: false,
            supports_responses_api: true,
            supports_chat_completions: false,
        },
        "gpt-5.4" => ModelTarget {
            model_id: "gpt-5.4".into(),
            supports_codex_backend: true,
            supports_responses_api: true,
            supports_chat_completions: true,
        },
        "gpt-5.3-codex" | "gpt-4o" | "gpt-4o-2024-11-20" | "gpt-4" | "gpt-4-turbo"
        | "gpt-4-turbo-preview" | "gpt-4o-mini" | "gpt-3.5-turbo"
        | "gpt-3.5-turbo-0125" => ModelTarget {
            model_id: "gpt-5.3-codex".into(),
            supports_codex_backend: true,
            supports_responses_api: true,
            supports_chat_completions: true,
        },
        "codex-mini" => ModelTarget {
            model_id: "codex-mini".into(),
            supports_codex_backend: false,
            supports_responses_api: true,
            supports_chat_completions: true,
        },
        m if m.starts_with("gpt-5.") || m.starts_with("codex") => ModelTarget {
            model_id: m.to_string(),
            supports_codex_backend: true,
            supports_responses_api: true,
            supports_chat_completions: true,
        },
        _ => ModelTarget {
            model_id: "gpt-5.3-codex".into(),
            supports_codex_backend: true,
            supports_responses_api: true,
            supports_chat_completions: true,
        },
    }
}

pub fn map_model(input: &str) -> String {
    resolve_model(input).model_id
}

// ---------------------------------------------------------------------------
// Tool format conversion
//
// Inbound (Chat Completions format):
//   { "type": "function", "function": { "name": ..., "description": ..., "parameters": ... } }
//
// Responses API format (both ChatGptCodex and OpenAiResponses):
//   { "type": "function", "name": ..., "description": ..., "parameters": ... }
//
// Chat Completions outbound: same as inbound, pass through unchanged.
// ---------------------------------------------------------------------------

/// Convert a Chat Completions tools array to Responses API format.
/// Unwraps the nested "function" wrapper.
pub fn convert_tools_to_responses_format(tools: &Value) -> Value {
    let Some(arr) = tools.as_array() else { return tools.clone() };
    let converted: Vec<Value> = arr
        .iter()
        .map(|tool| {
            if tool.get("type").and_then(Value::as_str) == Some("function") {
                if let Some(func) = tool.get("function") {
                    // Already in Responses API format if there's no nested "function" key.
                    // Re-flatten: move name/description/parameters up.
                    let mut out = serde_json::json!({ "type": "function" });
                    if let Some(name) = func.get("name") {
                        out["name"] = name.clone();
                    }
                    if let Some(desc) = func.get("description") {
                        out["description"] = desc.clone();
                    }
                    if let Some(params) = func.get("parameters") {
                        out["parameters"] = params.clone();
                    }
                    if let Some(strict) = func.get("strict") {
                        out["strict"] = strict.clone();
                    }
                    return out;
                }
            }
            tool.clone()
        })
        .collect();
    Value::Array(converted)
}

/// Convert a Chat Completions tool_choice to Responses API format.
/// Chat Completions: "auto" | "none" | "required" | {"type":"function","function":{"name":"..."}}
/// Responses API:    "auto" | "none" | "required" | {"type":"function","name":"..."}
pub fn convert_tool_choice_to_responses_format(tool_choice: &Value) -> Value {
    if let Some(obj) = tool_choice.as_object() {
        if obj.get("type").and_then(Value::as_str) == Some("function") {
            if let Some(func) = obj.get("function") {
                if let Some(name) = func.get("name") {
                    return serde_json::json!({ "type": "function", "name": name });
                }
            }
        }
    }
    tool_choice.clone()
}

// ---------------------------------------------------------------------------
// Responses API request types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ResponsesRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<ResponsesInputItem>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

/// Input item for the Responses API — can be a message or a tool result.
#[derive(Debug, Serialize, Clone)]
#[serde(untagged)]
pub enum ResponsesInputItem {
    /// Regular user/assistant message with content parts.
    Message(InputMessage),
    /// Function call output submitted by the client.
    FunctionCallOutput(FunctionCallOutput),
    /// Assistant function call recorded in history (so the model knows it called a tool).
    FunctionCall(FunctionCallItem),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputMessage {
    pub role: String,
    pub content: Vec<ContentPart>,
}

/// `{ "type": "function_call_output", "call_id": "...", "output": "..." }`
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCallOutput {
    #[serde(rename = "type")]
    pub item_type: String,
    pub call_id: String,
    pub output: String,
}

/// `{ "type": "function_call", "call_id": "...", "name": "...", "arguments": "..." }`
/// Used when an assistant message contained tool_calls — we re-submit the calls
/// so the model has context about what it already invoked.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCallItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
}

// ---------------------------------------------------------------------------
// Chat Completions outbound (Backend C)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ChatCompletionsOutbound {
    pub model: String,
    pub messages: Vec<Value>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

// ---------------------------------------------------------------------------
// Request conversion — profile-aware
// ---------------------------------------------------------------------------

fn resolve_model_id(req_model: &str, default_model: Option<&str>) -> String {
    let target = resolve_model(req_model);
    if let Some(dm) = default_model {
        let is_explicit = req_model.starts_with("gpt-5.") || req_model.starts_with("codex");
        if is_explicit { target.model_id } else { dm.to_string() }
    } else {
        target.model_id
    }
}

/// Convert an OpenAI Chat Completions request to the Responses API wire format.
pub fn convert_request(
    req: &ChatCompletionRequest,
    default_model: Option<&str>,
    profile: BackendProfile,
) -> ResponsesRequest {
    let model = resolve_model_id(&req.model, default_model);

    let mut system_parts: Vec<String> = Vec::new();
    let mut input_items: Vec<ResponsesInputItem> = Vec::new();

    for msg in &req.messages {
        match msg.role.as_str() {
            "system" => {
                system_parts.push(extract_text(&msg.content));
            }
            "tool" => {
                // Tool result message → function_call_output item.
                let call_id = msg.tool_call_id.clone().unwrap_or_default();
                let output = extract_text(&msg.content);
                input_items.push(ResponsesInputItem::FunctionCallOutput(FunctionCallOutput {
                    item_type: "function_call_output".to_string(),
                    call_id,
                    output,
                }));
            }
            "assistant" => {
                // If assistant message has tool_calls, emit function_call items first,
                // then any text content as an output_text part.
                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        input_items.push(ResponsesInputItem::FunctionCall(FunctionCallItem {
                            item_type: "function_call".to_string(),
                            call_id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        }));
                    }
                }
                let text = extract_text(&msg.content);
                if !text.is_empty() {
                    input_items.push(ResponsesInputItem::Message(InputMessage {
                        role: "assistant".to_string(),
                        content: vec![ContentPart {
                            part_type: "output_text".to_string(),
                            text,
                        }],
                    }));
                }
            }
            _ => {
                // user and any other roles
                input_items.push(ResponsesInputItem::Message(InputMessage {
                    role: msg.role.clone(),
                    content: vec![ContentPart {
                        part_type: "input_text".to_string(),
                        text: extract_text(&msg.content),
                    }],
                }));
            }
        }
    }

    if input_items.is_empty() {
        input_items.push(ResponsesInputItem::Message(InputMessage {
            role: "user".to_string(),
            content: vec![ContentPart {
                part_type: "input_text".to_string(),
                text: String::new(),
            }],
        }));
    }

    let instructions = if system_parts.is_empty() {
        Some("You are a helpful assistant.".to_string())
    } else {
        Some(system_parts.join("\n\n"))
    };

    // Convert tools to Responses API format (unwrap nested "function" wrapper).
    let tools = req.tools.as_ref().map(convert_tools_to_responses_format);
    let tool_choice = req.tool_choice.as_ref().map(convert_tool_choice_to_responses_format);

    // ChatGptCodex rejects temperature, top_p, max_output_tokens — omit them.
    let (temperature, top_p, max_output_tokens, stop, store, stream) = match profile {
        BackendProfile::ChatGptCodex => (None, None, None, None, Some(false), true),
        BackendProfile::OpenAiResponses => (
            req.temperature,
            req.top_p,
            req.max_tokens,
            req.stop.clone(),
            None,
            req.stream,
        ),
        BackendProfile::OpenAiChatCompletions => {
            unreachable!("Chat Completions path does not use convert_request()")
        }
    };

    ResponsesRequest {
        model,
        instructions,
        input: input_items,
        stream,
        max_output_tokens,
        temperature,
        top_p,
        stop,
        store,
        tools,
        tool_choice,
        parallel_tool_calls: req.parallel_tool_calls,
    }
}

/// Convert an OpenAI Chat Completions request to the Chat Completions outbound format.
/// Messages are serialized to Value to preserve tool_calls and tool role fields verbatim.
pub fn build_chat_completions_request(
    req: &ChatCompletionRequest,
    default_model: Option<&str>,
) -> ChatCompletionsOutbound {
    let model = resolve_model_id(&req.model, default_model);

    // Serialize messages to Value, preserving all fields (tool_calls, tool_call_id, etc.)
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::json!({})))
        .collect();

    ChatCompletionsOutbound {
        model,
        messages,
        stream: req.stream,
        max_completion_tokens: req.max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        stop: req.stop.clone(),
        // Tools pass through unchanged — Chat Completions format is already native.
        tools: req.tools.clone(),
        tool_choice: req.tool_choice.clone(),
        parallel_tool_calls: req.parallel_tool_calls,
    }
}

fn extract_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(parts) => parts
            .iter()
            .filter(|p| p.part_type == "text")
            .filter_map(|p| p.text.as_deref())
            .collect::<Vec<_>>()
            .join(""),
        MessageContent::Null => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Responses API — SSE event types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated { response: ResponseCreatedData },

    #[serde(rename = "response.in_progress")]
    ResponseInProgress {},

    /// Fired when a new output item (text block or function_call) starts.
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded {
        output_index: u32,
        item: OutputItemMeta,
    },

    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded {
        output_index: u32,
        content_index: u32,
        part: ContentPartMeta,
    },

    /// Text delta for a text output item.
    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta {
        output_index: u32,
        content_index: u32,
        delta: String,
    },

    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone {
        output_index: u32,
        content_index: u32,
        text: String,
    },

    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone {
        output_index: u32,
        content_index: u32,
        part: ContentPartMeta,
    },

    /// Streaming delta for a function_call's arguments.
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta {
        output_index: u32,
        delta: String,
    },

    /// Final accumulated arguments for a function_call.
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone {
        output_index: u32,
        arguments: String,
    },

    /// Fired when an output item (text or function_call) is complete.
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone {
        output_index: u32,
        item: OutputItemFull,
    },

    #[serde(rename = "response.completed")]
    ResponseDone { response: ResponseDoneData },

    #[serde(other)]
    Unknown,
}

/// Lightweight metadata when an output item first appears.
#[derive(Debug, Deserialize)]
pub struct OutputItemMeta {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    /// Present for function_call items.
    pub name: Option<String>,
    pub call_id: Option<String>,
}

/// Full output item once it is done — carries all fields for function_call.
#[derive(Debug, Deserialize)]
pub struct OutputItemFull {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    /// Function name (function_call items).
    pub name: Option<String>,
    /// Stable call_id reference (function_call items).
    pub call_id: Option<String>,
    /// Fully accumulated arguments JSON string (function_call items).
    pub arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContentPartMeta {
    #[serde(rename = "type")]
    pub part_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ResponseCreatedData {
    pub id: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct ResponseDoneData {
    pub id: String,
    pub model: String,
    pub usage: Option<ResponseUsage>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

pub fn map_finish_reason(status: Option<&str>) -> String {
    match status {
        Some("completed") => "stop".to_string(),
        Some("max_tokens") | Some("length") | Some("incomplete") => "length".to_string(),
        Some("tool_calls") => "tool_calls".to_string(),
        Some(other) => other.to_string(),
        None => "stop".to_string(),
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible streaming response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool call deltas — present when the model is streaming function call arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// A streaming tool call delta within a chunk.
#[derive(Debug, Serialize, Clone)]
pub struct ToolCallDelta {
    /// Position index within the tool_calls array.
    pub index: u32,
    /// Only present on the first chunk for this call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Always "function".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub call_type: Option<&'static str>,
    pub function: ToolCallFunctionDelta,
}

#[derive(Debug, Serialize, Clone)]
pub struct ToolCallFunctionDelta {
    /// Only present on the first chunk for this call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Partial or complete arguments JSON string.
    pub arguments: String,
}

// ---------------------------------------------------------------------------
// Pending function-call accumulator (used by proxy.rs non-streaming path)
// ---------------------------------------------------------------------------

/// Accumulates a single function call from Responses API SSE events.
#[derive(Debug, Default, Clone)]
pub struct PendingFunctionCall {
    pub output_index: u32,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

impl PendingFunctionCall {
    pub fn into_tool_call(self) -> ToolCall {
        ToolCall {
            id: self.call_id,
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: self.name,
                arguments: self.arguments,
            },
        }
    }
}
