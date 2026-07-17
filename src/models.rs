use axum::{Json, extract::State};
use serde::Serialize;

use crate::{AppState, codex::BackendProfile, model_catalog};

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

pub async fn list_models(State(state): State<AppState>) -> Json<ModelList> {
    let profile = state.backend_profile;
    let data = model_catalog::CATALOG
        .iter()
        .filter(|entry| entry.supports(profile))
        .map(|entry| model_object(entry, profile))
        .collect();

    Json(ModelList {
        object: "list",
        data,
    })
}

fn model_object(entry: &model_catalog::ModelCatalogEntry, profile: BackendProfile) -> ModelObject {
    ModelObject {
        id: entry.model_id.to_string(),
        object: "model",
        created: 1_700_000_000,
        owned_by: "openai-proxy",
        context_length: entry.context_length_for(profile),
        max_output_tokens: entry.max_output_tokens,
    }
}
