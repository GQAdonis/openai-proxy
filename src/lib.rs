pub mod a2a;
pub mod acp;
pub mod agui;
pub mod cli;
pub mod config;
pub mod codex;
pub mod error;
pub mod hooks;
pub mod mcp;
pub mod mcp_client;
pub mod memory;
pub mod models;
pub mod openai;
pub mod proxy;
pub mod skills;

use std::sync::Arc;

use axum::{Router, routing::get, routing::post};
use tower_http::cors::CorsLayer;

use crate::a2a::agent_card_handler;
use crate::agui::agui_stream;
use crate::codex::{BackendProfile, CodexAuth, CODEX_BACKEND_URL, OPENAI_RESPONSES_URL};
use crate::hooks::{NullHooks, ProxyHooks};
use crate::mcp_client::McpToolSchema;
use crate::memory::DynMemory;
#[cfg(feature = "memory")]
use crate::memory::MemoryStore;
use crate::skills::SkillManifest;

#[derive(Clone)]
pub struct AppState {
    pub auth: CodexAuth,
    pub backend_url: String,
    pub backend_profile: BackendProfile,
    pub http_client: reqwest::Client,
    pub default_model: Option<String>,
    /// Hook implementation — defaults to `NullHooks` (no-op).
    pub hooks: Arc<dyn ProxyHooks + Send + Sync>,
    /// The proxy's own listen address (e.g. "127.0.0.1:8080") — used in the A2A Agent Card.
    pub bind_addr: String,
    /// Skills loaded from `PROXY_SKILLS_DIRS` at startup.
    pub skills: Arc<Vec<SkillManifest>>,
    /// MCP tool schemas for passthrough injection.
    pub mcp_tools: Arc<Vec<McpToolSchema>>,
    /// Memory backend (noop unless `--features memory` and `memory.enabled = true`).
    pub memory: DynMemory,
    /// Concrete memory store — only present when `--features memory` and memory is enabled.
    #[cfg(feature = "memory")]
    pub memory_store: Option<Arc<MemoryStore>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("backend_url", &self.backend_url)
            .field("backend_profile", &self.backend_profile)
            .field("default_model", &self.default_model)
            .field("mcp_tools", &self.mcp_tools.len())
            .field("memory_enabled", &self.memory.is_enabled())
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
        .route("/ag-ui/stream", post(agui_stream))
        .route("/health", get(health));

    if enable_a2a {
        router = router.route("/.well-known/agent.json", get(agent_card_handler));
    }

    #[cfg(feature = "memory")]
    {
        use crate::memory::handlers as mem;
        use axum::routing::delete;
        router = router
            .route("/v1/memory/documents", post(mem::create_document))
            .route("/v1/memory/documents", get(mem::list_documents))
            .route("/v1/memory/documents/:id", delete(mem::delete_document))
            .route("/v1/memory/search", get(mem::search_documents));
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
        bind_addr: "127.0.0.1:8080".to_string(),
        skills: Arc::new(Vec::new()),
        mcp_tools: Arc::new(Vec::new()),
        memory: crate::memory::noop(),
        #[cfg(feature = "memory")]
        memory_store: None,
    };

    (state, backend_url)
}
