use agent_client_protocol::{
    Agent, ByteStreams, Client, ConnectionTo,
    schema::{
        AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
        NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
        SessionNotification, SessionUpdate, StopReason, TextContent,
    },
};
use futures_util::StreamExt;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{
    AppState,
    codex::{self, ResponseStreamEvent},
    openai::{ChatCompletionRequest, Message, MessageContent},
    proxy::build_responses_request,
    skills::select_skills,
};

/// Run the ACP stdio server. Reads JSON-RPC 2.0 from stdin, dispatches prompt
/// requests to the proxy backend, streams responses back as ACP session
/// notifications, and writes JSON-RPC to stdout.
///
/// All logging goes to stderr — stdout is reserved for JSON-RPC only.
pub async fn run_acp_server(state: AppState) -> anyhow::Result<()> {
    tracing::info!(
        profile = state.backend_profile.name(),
        "starting ACP stdio server"
    );

    Agent
        .builder()
        .name("openai-proxy")
        .on_receive_request(
            {
                let _state = state.clone();
                async move |req: InitializeRequest,
                            responder,
                            _cx: ConnectionTo<Client>| {
                    responder.respond(
                        InitializeResponse::new(req.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: NewSessionRequest,
                        responder,
                        _cx: ConnectionTo<Client>| {
                let session_id = SessionId::new(uuid::Uuid::new_v4().to_string());
                tracing::debug!(
                    session = %session_id,
                    cwd = %req.cwd.display(),
                    "new ACP session"
                );
                responder.respond(NewSessionResponse::new(session_id))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = state.clone();
                async move |req: PromptRequest,
                            responder,
                            cx: ConnectionTo<Client>| {
                    let session_id = req.session_id.clone();
                    match handle_prompt(req, &state, &cx).await {
                        Ok(stop_reason) => responder.respond(PromptResponse::new(stop_reason)),
                        Err(e) => {
                            tracing::error!(
                                session = %session_id,
                                error = %e,
                                "ACP prompt handler error"
                            );
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(ByteStreams::new(
            tokio::io::stdout().compat_write(),
            tokio::io::stdin().compat(),
        ))
        .await
        .map_err(|e| anyhow::anyhow!("ACP server error: {e}"))
}

/// Extract plain text from ACP `ContentBlock` items.
fn extract_prompt_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(t) = block {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Handle a single `session/prompt` request:
/// 1. Convert the ACP prompt to an OpenAI `ChatCompletionRequest`
/// 2. Apply skill injection
/// 3. Forward to the configured backend (Responses API or Chat Completions)
/// 4. Stream `SessionNotification` chunks back to the ACP client
/// 5. Return the `StopReason`
async fn handle_prompt(
    req: PromptRequest,
    state: &AppState,
    cx: &ConnectionTo<Client>,
) -> anyhow::Result<StopReason> {
    let session_id = req.session_id.clone();
    let user_text = extract_prompt_text(&req.prompt);

    tracing::debug!(session = %session_id, chars = user_text.len(), "ACP prompt received");

    // Build a minimal ChatCompletionRequest from the ACP prompt.
    let user_msg = Message {
        role: "user".to_string(),
        content: MessageContent::Text(user_text.clone()),
        tool_call_id: None,
        tool_calls: None,
        name: None,
    };

    let mut messages = vec![user_msg];

    // Skill injection: prepend selected skills as a system message.
    if !state.skills.is_empty() {
        let max = std::env::var("PROXY_SKILLS_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(3);
        let selected = select_skills(&user_text, &state.skills, max, state.backend_profile.name());
        if !selected.is_empty() {
            let skill_block = selected
                .iter()
                .map(|s| s.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            messages.insert(
                0,
                Message {
                    role: "system".to_string(),
                    content: MessageContent::Text(format!("# Active Skills\n\n{skill_block}")),
                    tool_call_id: None,
                    tool_calls: None,
                    name: None,
                },
            );
        }
    }

    let chat_req = ChatCompletionRequest {
        model: state
            .default_model
            .clone()
            .unwrap_or_else(|| "gpt-5.3-codex".to_string()),
        messages,
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

    // Convert to Responses API wire format and send.
    let codex_req =
        codex::convert_request(&chat_req, state.default_model.as_deref(), state.backend_profile);

    let http_req = build_responses_request(state, &codex_req)
        .map_err(|e| anyhow::anyhow!("failed to build request: {e}"))?;

    let resp = http_req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("upstream HTTP error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("upstream {status}: {body}"));
    }

    // Incrementally consume the SSE stream — never buffer the full body.
    let mut stream = resp.bytes_stream();
    let mut remainder = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| anyhow::anyhow!("upstream stream error: {e}"))?;
        remainder.push_str(&String::from_utf8_lossy(&bytes));

        // Process all complete lines in the accumulated buffer.
        loop {
            let Some(nl) = remainder.find('\n') else { break };
            let line: String = remainder.drain(..=nl).collect();
            let line = line.trim_end_matches(['\n', '\r']);

            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                return Ok(StopReason::EndTurn);
            }
            let Ok(event) = serde_json::from_str::<ResponseStreamEvent>(data) else {
                continue;
            };
            if let ResponseStreamEvent::ResponseOutputTextDelta { delta, .. } = event {
                if delta.is_empty() {
                    continue;
                }
                let notif = SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(delta),
                    ))),
                );
                if let Err(e) = cx.send_notification(notif) {
                    tracing::warn!(error = %e, "failed to send ACP chunk notification");
                    return Ok(StopReason::EndTurn);
                }
            }
        }
    }

    Ok(StopReason::EndTurn)
}
