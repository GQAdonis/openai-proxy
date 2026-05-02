/// AG-UI (Agent-User Interaction) streaming endpoint.
///
/// Implements the 5-event AG-UI protocol over Server-Sent Events so that
/// AG-UI-aware frontends (e.g. CopilotKit) can stream responses from this
/// proxy without polling.
///
/// When an official `ag-ui-protocol` Rust crate is published (tracked in
/// ag-ui-protocol/ag-ui#239), these types can be replaced with that crate's
/// definitions.  The wire format here is intentionally compatible.
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response, Sse},
    response::sse::Event,
};
use futures_util::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;

use crate::{
    AppState,
    codex::{self, ResponseStreamEvent},
    openai::{ChatCompletionRequest, Message},
    proxy::build_responses_request,
};

// ── AG-UI event types ────────────────────────────────────────────────────────

/// The five AG-UI lifecycle events emitted on `POST /ag-ui/stream`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AguiEvent {
    /// Emitted once at the start of a run.
    RunStarted { run_id: String },
    /// Emitted once when the assistant begins generating text.
    TextMessageStart { message_id: String },
    /// Emitted for each incremental text chunk.
    TextMessageContent { message_id: String, delta: String },
    /// Emitted once when text generation finishes.
    TextMessageEnd { message_id: String },
    /// Emitted once at the end of a run.
    RunFinished { run_id: String },
}

impl AguiEvent {
    fn to_sse_event(&self) -> Result<Event, serde_json::Error> {
        let json = serde_json::to_string(self)?;
        Ok(Event::default().data(json))
    }
}

// ── Request / response ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AguiRequest {
    /// Standard OpenAI-style messages.
    pub messages: Vec<Message>,
    /// Optional model override.
    pub model: Option<String>,
}

// ── Handler ──────────────────────────────────────────────────────────────────

/// `POST /ag-ui/stream`
///
/// Accepts an AG-UI request body and returns an SSE stream of AG-UI events.
/// The stream is compatible with the CopilotKit `useCoAgent` hook.
pub async fn agui_stream(
    State(state): State<AppState>,
    Json(body): Json<AguiRequest>,
) -> Response {
    let run_id = uuid::Uuid::new_v4().to_string();
    let message_id = uuid::Uuid::new_v4().to_string();

    let model = body
        .model
        .or_else(|| state.default_model.clone())
        .unwrap_or_else(|| "gpt-5.3-codex".to_string());

    let chat_req = ChatCompletionRequest {
        model,
        messages: body.messages,
        stream: true,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        system: None,
        user: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
    };

    let sse_stream = build_agui_stream(state, chat_req, run_id, message_id).await;
    Sse::new(sse_stream).into_response()
}

async fn build_agui_stream(
    state: AppState,
    chat_req: ChatCompletionRequest,
    run_id: String,
    message_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let run_id_end = run_id.clone();
    let message_id_end = message_id.clone();

    let codex_req =
        codex::convert_request(&chat_req, state.default_model.as_deref(), state.backend_profile);

    // Build the upstream request; on failure emit a single error run.
    let http_req = match build_responses_request(&state, &codex_req) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "ag-ui: failed to build upstream request");
            return stream::iter(vec![
                Ok(Event::default().data(format!(r#"{{"type":"RUN_STARTED","run_id":"{run_id}"}}"#))),
                Ok(Event::default().data(format!(r#"{{"type":"RUN_FINISHED","run_id":"{run_id_end}"}}"#))),
            ])
            .left_stream();
        }
    };

    let resp = match http_req.send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            tracing::error!(status = %status, "ag-ui: upstream error");
            return stream::iter(vec![
                Ok(Event::default().data(format!(r#"{{"type":"RUN_STARTED","run_id":"{run_id}"}}"#))),
                Ok(Event::default().data(format!(r#"{{"type":"RUN_FINISHED","run_id":"{run_id_end}"}}"#))),
            ])
            .left_stream();
        }
        Err(e) => {
            tracing::error!(error = %e, "ag-ui: upstream HTTP error");
            return stream::iter(vec![
                Ok(Event::default().data(format!(r#"{{"type":"RUN_STARTED","run_id":"{run_id}"}}"#))),
                Ok(Event::default().data(format!(r#"{{"type":"RUN_FINISHED","run_id":"{run_id_end}"}}"#))),
            ])
            .left_stream();
        }
    };

    // Stream preamble + body chunks + epilogue.
    let preamble: Vec<Result<Event, Infallible>> = vec![
        Ok(AguiEvent::RunStarted { run_id: run_id.clone() }
            .to_sse_event()
            .unwrap_or_else(|_| Event::default())),
        Ok(AguiEvent::TextMessageStart { message_id: message_id.clone() }
            .to_sse_event()
            .unwrap_or_else(|_| Event::default())),
    ];

    let body_stream = resp
        .bytes_stream()
        .scan(String::new(), move |remainder, chunk| {
            let message_id = message_id.clone();
            let chunk = match chunk {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, "ag-ui: stream chunk error");
                    return std::future::ready(Some(vec![]));
                }
            };
            remainder.push_str(&String::from_utf8_lossy(&chunk));
            let mut events = Vec::new();
            loop {
                let Some(nl) = remainder.find('\n') else { break };
                let line: String = remainder.drain(..=nl).collect();
                let line = line.trim_end_matches(['\n', '\r']);
                let Some(data) = line.strip_prefix("data: ") else { continue };
                if data == "[DONE]" {
                    break;
                }
                let Ok(event) = serde_json::from_str::<ResponseStreamEvent>(data) else {
                    continue;
                };
                if let ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } = event {
                    if !delta.is_empty() {
                        let ev = AguiEvent::TextMessageContent {
                            message_id: message_id.clone(),
                            delta,
                        }
                        .to_sse_event()
                        .unwrap_or_else(|_| Event::default());
                        events.push(Ok(ev));
                    }
                }
            }
            std::future::ready(Some(events))
        })
        .flat_map(stream::iter);

    let epilogue: Vec<Result<Event, Infallible>> = vec![
        Ok(AguiEvent::TextMessageEnd { message_id: message_id_end.clone() }
            .to_sse_event()
            .unwrap_or_else(|_| Event::default())),
        Ok(AguiEvent::RunFinished { run_id: run_id_end }
            .to_sse_event()
            .unwrap_or_else(|_| Event::default())),
    ];

    stream::iter(preamble)
        .chain(body_stream)
        .chain(stream::iter(epilogue))
        .right_stream()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agui_event_serializes_correctly() {
        let ev = AguiEvent::RunStarted { run_id: "abc".to_string() };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"RUN_STARTED\""));
        assert!(json.contains("\"run_id\":\"abc\""));
    }

    #[test]
    fn agui_text_content_serializes_correctly() {
        let ev = AguiEvent::TextMessageContent {
            message_id: "m1".to_string(),
            delta: "hello".to_string(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"TEXT_MESSAGE_CONTENT\""));
        assert!(json.contains("\"delta\":\"hello\""));
    }

    #[test]
    fn agui_run_finished_serializes_correctly() {
        let ev = AguiEvent::RunFinished { run_id: "r1".to_string() };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"RUN_FINISHED\""));
    }
}
