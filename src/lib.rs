pub mod a2a;
pub mod acp;
pub mod agui;
pub mod auth;
pub mod cli;
pub mod codex;
pub mod config;
pub mod console;
pub mod error;
pub mod hooks;
pub mod mcp;
pub mod mcp_client;
pub mod memory;
pub mod model_catalog;
pub mod models;
pub mod openai;
pub mod proxy;
pub mod skills;
pub mod web_assets;

use std::sync::Arc;

use axum::{Router, routing::get, routing::post};
use tower_http::cors::CorsLayer;

use crate::a2a::agent_card_handler;
use crate::agui::agui_stream;
use crate::codex::{BackendProfile, CodexAuth};
use crate::hooks::{NullHooks, ProxyHooks};
use crate::mcp_client::McpToolSchema;
use crate::memory::DynMemory;
#[cfg(feature = "memory")]
use crate::memory::MemoryStore;
use crate::skills::SkillIndex;

#[derive(Clone)]
pub struct AppState {
    pub credentials: auth::CredentialProvider,
    pub runtime: Arc<console::RuntimeInfo>,
    // These fields are populated together only on a request clone or by the live-test factory.
    #[doc(hidden)]
    pub auth: CodexAuth,
    #[doc(hidden)]
    pub generation: u64,
    #[doc(hidden)]
    pub owner_epoch: u64,
    pub backend_url: String,
    pub backend_profile: BackendProfile,
    pub http_client: reqwest::Client,
    pub default_model: Option<String>,
    /// Hook implementation — defaults to `NullHooks` (no-op).
    pub hooks: Arc<dyn ProxyHooks + Send + Sync>,
    /// The proxy's own listen address (e.g. "127.0.0.1:8181") — used in the A2A Agent Card.
    pub bind_addr: String,
    /// Skills loaded from `PROXY_SKILLS_DIRS` at startup.
    pub skills: Arc<SkillIndex>,
    /// MCP tool schemas for passthrough injection.
    pub mcp_tools: Arc<Vec<McpToolSchema>>,
    /// Memory backend (noop unless `--features memory` and `memory.enabled = true`).
    pub memory: DynMemory,
    /// Concrete memory store — only present when `--features memory` and memory is enabled.
    #[cfg(feature = "memory")]
    pub memory_store: Option<Arc<MemoryStore>>,
}

impl AppState {
    pub async fn for_request(&self) -> Result<Self, error::ProxyError> {
        let identity = self.credentials.snapshot().await?;
        let mut state = self.clone();
        state.auth = identity.auth;
        state.backend_url = identity.backend_url;
        state.backend_profile = identity.backend_profile;
        state.generation = identity.generation;
        state.owner_epoch = identity.owner_epoch;
        Ok(state)
    }

    pub(crate) async fn send_upstream(
        &self,
        builder: reqwest::RequestBuilder,
        retry_safe: bool,
    ) -> Result<reqwest::Response, error::ProxyError> {
        let req = builder.build()?;
        let retry = if retry_safe { req.try_clone() } else { None };
        if !self.credentials.is_current(self.generation).await {
            return Err(error::ProxyError::AuthUnavailable(
                "credential changed before dispatch".into(),
            ));
        }
        let response = self.http_client.execute(req).await?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            if response.status().is_success() {
                self.credentials.authenticated(self.generation);
            }
            return Ok(response);
        }
        let challenge = response
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let content = response.bytes().await?;
        let error_code = serde_json::from_slice::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v["error"]["code"].as_str().map(str::to_owned));
        let token_denied = challenge.contains("invalid_token")
            || matches!(
                error_code.as_deref(),
                Some(
                    "invalid_token"
                        | "token_expired"
                        | "expired_token"
                        | "invalid_api_key"
                        | "token_revoked"
                )
            );
        let denied = || error::ProxyError::Upstream {
            status: 401,
            body: "Upstream authentication denied".into(),
        };
        if !token_denied {
            return Err(denied());
        }
        self.credentials.rejected(self.generation).await;
        let Some(mut req) = retry else {
            return Err(denied());
        };
        let replacement = self
            .credentials
            .snapshot_for(Some(self.owner_epoch))
            .await
            .map_err(|_| denied())?;
        if replacement.owner_epoch != self.owner_epoch
            || replacement.generation == self.generation
            || replacement.backend_profile != self.backend_profile
            || replacement.backend_url != self.backend_url
        {
            return Err(denied());
        }
        let (bearer, _) = replacement.auth.bearer()?;
        let value = reqwest::header::HeaderValue::from_str(&bearer).map_err(|_| denied())?;
        req.headers_mut()
            .insert(reqwest::header::AUTHORIZATION, value);
        if !self.credentials.is_current(replacement.generation).await {
            return Err(denied());
        }
        let response = self.http_client.execute(req).await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(denied());
        }
        if response.status().is_success() {
            self.credentials.authenticated(replacement.generation);
        }
        Ok(response)
    }
}

pub(crate) fn upstream_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 => "Upstream authentication denied",
        403 => "Upstream denied access to this resource",
        404 => "Requested upstream resource was not found",
        429 => "Upstream quota or rate limit reached",
        500..=599 => "Upstream service unavailable",
        _ => "Upstream rejected the request",
    }
    .into()
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
    let legacy = direct_app(state.clone(), enable_a2a);
    crate::console::router_with_legacy(state, legacy, enable_a2a)
}

pub(crate) fn direct_app(state: AppState, enable_a2a: bool) -> Router {
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
            .route("/v1/memory/documents/{id}", delete(mem::delete_document))
            .route("/v1/memory/search", get(mem::search_documents));
    }

    router.layer(CorsLayer::permissive()).with_state(state)
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

/// Load the caller-selected real credentials for the opt-in integration suite.
/// Panics if the file is missing or malformed — tests require real credentials.
pub fn load_real_auth() -> (AppState, String) {
    let wire_api = std::env::var("CODEX_WIRE_API").unwrap_or_default();
    let credentials =
        auth::CredentialProvider::process(None, &wire_api, std::env::var("CODEX_BACKEND_URL").ok())
            .expect("selected credential provider");
    let observed = credentials
        .observed_snapshot(None)
        .expect("live integration needs a usable selected credential");
    let backend_url = observed.backend_url.clone();
    let backend_profile = observed.backend_profile;
    let auth = observed.auth;
    let state = AppState {
        credentials,
        runtime: Arc::default(),
        generation: 0,
        owner_epoch: 0,
        auth,
        backend_url: backend_url.clone(),
        backend_profile,
        http_client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("http client"),
        default_model: None,
        hooks: Arc::new(NullHooks),
        bind_addr: "127.0.0.1:8181".to_string(),
        skills: Arc::new(SkillIndex::build(vec![])),
        mcp_tools: Arc::new(Vec::new()),
        memory: crate::memory::noop(),
        #[cfg(feature = "memory")]
        memory_store: None,
    };

    (state, backend_url)
}
