pub mod a2a;
pub mod codex;
pub mod error;
pub mod hooks;
pub mod mcp;
pub mod models;
pub mod openai;
pub mod proxy;

use std::sync::Arc;

use axum::{Router, routing::get, routing::post};
use tower_http::cors::CorsLayer;

use crate::a2a::agent_card_handler;
use crate::codex::{BackendProfile, CodexAuth, CODEX_BACKEND_URL, OPENAI_RESPONSES_URL};
use crate::hooks::{NullHooks, ProxyHooks};

#[derive(Clone)]
pub struct AppState {
    pub auth: CodexAuth,
    pub backend_url: String,
    pub backend_profile: BackendProfile,
    pub http_client: reqwest::Client,
    pub default_model: Option<String>,
    /// Hook implementation — defaults to `NullHooks` (no-op).
    pub hooks: Arc<dyn ProxyHooks + Send + Sync>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("backend_url", &self.backend_url)
            .field("backend_profile", &self.backend_profile)
            .field("default_model", &self.default_model)
            .finish_non_exhaustive()
    }
}

/// Build the axum Router from a given AppState (shared by main and tests).
///
/// When `enable_a2a` is `true`, mounts `GET /.well-known/agent.json` →
/// [`agent_card_handler`] for A2A orchestrator discoverability.
pub fn build_app(state: AppState, enable_a2a: bool) -> Router {
    let mut router = Router::new()
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/v1/models", get(models::list_models))
        .route("/health", get(health));

    if enable_a2a {
        router = router.route("/.well-known/agent.json", get(agent_card_handler));
    }

    router.layer(CorsLayer::permissive()).with_state(state)
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

/// Load real credentials from the standard ~/.codex/auth.json location.
/// Panics if the file is missing or malformed — tests require real credentials.
pub fn load_real_auth() -> (AppState, String) {
    let auth_path = dirs::home_dir()
        .expect("cannot determine home dir")
        .join(".codex")
        .join("auth.json");

    let auth = CodexAuth::load(&auth_path)
        .unwrap_or_else(|e| panic!("~/.codex/auth.json missing or invalid: {e}\nRun `codex login` first."));

    let wire_api = std::env::var("CODEX_WIRE_API").unwrap_or_default();
    let (backend_url, backend_profile) = if auth.access_token.is_some() {
        (CODEX_BACKEND_URL.to_string(), BackendProfile::ChatGptCodex)
    } else if wire_api.eq_ignore_ascii_case("chat") {
        (crate::codex::OPENAI_CHAT_URL.to_string(), BackendProfile::OpenAiChatCompletions)
    } else {
        (OPENAI_RESPONSES_URL.to_string(), BackendProfile::OpenAiResponses)
    };

    let state = AppState {
        auth,
        backend_url: backend_url.clone(),
        backend_profile,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("http client"),
        default_model: None,
        hooks: Arc::new(NullHooks),
    };

    (state, backend_url)
}
