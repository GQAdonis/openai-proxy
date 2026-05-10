use axum::{Json, extract::State};
use serde::Serialize;

use crate::{AppState, codex::BackendProfile};

#[derive(Debug, Serialize)]
pub struct ModelList {
    pub object: &'static str,
    pub data: Vec<ModelObject>,
}

#[derive(Debug, Serialize)]
pub struct ModelObject {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub owned_by: &'static str,
    pub context_length: u32,
    pub max_output_tokens: u32,
}

struct ModelSpec {
    id: &'static str,
    context_length: u32,
    max_output_tokens: u32,
}

const fn spec(id: &'static str, context_length: u32, max_output_tokens: u32) -> ModelSpec {
    ModelSpec { id, context_length, max_output_tokens }
}

pub async fn list_models(State(state): State<AppState>) -> Json<ModelList> {
    let models: &[ModelSpec] = match state.backend_profile {
        BackendProfile::ChatGptCodex => &[
            spec("gpt-5.5",       400_000, 32_768),
            spec("gpt-5.4",       400_000, 32_768),
            spec("gpt-5.4-mini",  200_000, 16_384),
            spec("gpt-5.4-nano",  128_000,  8_192),
            spec("gpt-5.3-codex", 400_000, 32_768),
            spec("gpt-5.3-chat",  128_000, 16_384),
            spec("gpt-5.2-chat",  128_000, 16_384),
        ],
        BackendProfile::OpenAiResponses => &[
            spec("gpt-5.5",       1_000_000, 32_768),
            spec("gpt-5.5-pro",   1_000_000, 32_768),
            spec("gpt-5.4",         400_000, 32_768),
            spec("gpt-5.4-mini",    200_000, 16_384),
            spec("gpt-5.4-nano",    128_000,  8_192),
            spec("gpt-5.3-codex",   400_000, 32_768),
            spec("gpt-5.3-chat",    128_000, 16_384),
            spec("gpt-5.2-chat",    128_000, 16_384),
        ],
        BackendProfile::OpenAiChatCompletions => &[
            spec("gpt-5.5",       1_000_000, 32_768),
            spec("gpt-5.5-pro",   1_000_000, 32_768),
            spec("gpt-5.4",         400_000, 32_768),
            spec("gpt-5.4-mini",    200_000, 16_384),
            spec("gpt-5.4-nano",    128_000,  8_192),
            spec("gpt-5.3-codex",   400_000, 32_768),
            spec("gpt-5.3-chat",    128_000, 16_384),
            spec("gpt-5.2-chat",    128_000, 16_384),
        ],
    };

    Json(ModelList {
        object: "list",
        data: models.iter().map(model_object).collect(),
    })
}

fn model_object(s: &ModelSpec) -> ModelObject {
    ModelObject {
        id: s.id.to_string(),
        object: "model",
        created: 1_700_000_000,
        owned_by: "openai-proxy",
        context_length: s.context_length,
        max_output_tokens: s.max_output_tokens,
    }
}
