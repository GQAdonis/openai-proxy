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

/// Axum handler — constructs and returns the Agent Card JSON.
pub async fn agent_card_handler(State(state): State<AppState>) -> Json<AgentCard> {
    let card = AgentCard {
        name: "openai-proxy".to_string(),
        description: "OpenAI Chat Completions proxy backed by Codex/Responses API".to_string(),
        url: format!("http://{}", state.bind_addr),
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
