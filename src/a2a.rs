use axum::{Json, extract::State};
use serde::Serialize;

use crate::AppState;

/// A2A Agent Card — static JSON document for agent discoverability.
/// Served at `GET /.well-known/agent.json` when the `--a2a` flag is set.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub capabilities: AgentCapabilities,
    pub skills: Vec<AgentSkill>,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub tools: bool,
    pub multi_turn: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
}

pub fn advertised(state: &AppState) -> String {
    if let Some(origin) = std::env::var("PROXY_CONSOLE_ORIGIN")
        .ok()
        .and_then(|raw| reqwest::Url::parse(&raw).ok())
        && origin.username().is_empty()
        && origin.password().is_none()
        && (origin.scheme() == "https"
            || (origin.scheme() == "http"
                && matches!(origin.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
    {
        return origin.origin().ascii_serialization();
    }
    match state.bind_addr.parse::<std::net::SocketAddr>() {
        Ok(addr) if addr.ip().is_unspecified() => format!("http://127.0.0.1:{}", addr.port()),
        _ => format!("http://{}", state.bind_addr),
    }
}

/// Axum handler — constructs and returns the Agent Card JSON.
pub async fn agent_card_handler(State(state): State<AppState>) -> Json<AgentCard> {
    let card = AgentCard {
        name: "openai-proxy".to_string(),
        description: "OpenAI Chat Completions proxy backed by Codex/Responses API".to_string(),
        url: advertised(&state),
        version: env!("CARGO_PKG_VERSION").to_string(),
        capabilities: AgentCapabilities {
            streaming: true,
            tools: true,
            multi_turn: true,
        },
        skills: vec![
            AgentSkill {
                id: "chat_completion".to_string(),
                name: "Chat Completion".to_string(),
                description: "Complete a conversation using the configured model backend"
                    .to_string(),
            },
            AgentSkill {
                id: "list_models".to_string(),
                name: "List Models".to_string(),
                description: "List available models for the configured backend profile".to_string(),
            },
        ],
        input_modes: vec!["text".to_string()],
        output_modes: vec!["text".to_string(), "stream".to_string()],
    };

    Json(card)
}
