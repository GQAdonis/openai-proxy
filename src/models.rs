use axum::{Extension, Json, extract::State};
use serde::Serialize;

use crate::{AppState, codex::BackendProfile, console::NativeModelDiscovery, model_catalog};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

pub async fn list_models(
    State(state): State<AppState>,
    discovery: Option<Extension<NativeModelDiscovery>>,
) -> Result<Json<ModelList>, crate::error::ProxyError> {
    let state = state.for_request().await?;
    if let Some(Extension(discovery)) = discovery
        && let Some(models) = discovery.models(&state).await
    {
        let data = models
            .into_iter()
            .filter_map(|row| {
                Some(ModelObject {
                    id: row["id"].as_str()?.to_owned(),
                    object: "model",
                    created: 1_700_000_000,
                    owned_by: "openai-proxy",
                    context_length: row["context_length"]
                        .as_u64()
                        .and_then(|limit| limit.try_into().ok()),
                    max_output_tokens: row["max_output_tokens"]
                        .as_u64()
                        .and_then(|limit| limit.try_into().ok()),
                })
            })
            .collect();
        return Ok(Json(ModelList {
            object: "list",
            data,
        }));
    }

    let profile = state.backend_profile;
    let data = model_catalog::CATALOG
        .iter()
        .filter(|entry| entry.supports(profile))
        .map(|entry| model_object(entry, profile))
        .collect();

    Ok(Json(ModelList {
        object: "list",
        data,
    }))
}

fn model_object(entry: &model_catalog::ModelCatalogEntry, profile: BackendProfile) -> ModelObject {
    ModelObject {
        id: entry.model_id.to_string(),
        object: "model",
        created: 1_700_000_000,
        owned_by: "openai-proxy",
        context_length: Some(entry.context_length_for(profile)),
        max_output_tokens: Some(entry.max_output_tokens),
    }
}
