use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt};

use openai_proxy_lib::{
    AppState, build_app,
    codex::{BackendProfile, CodexAuth, CODEX_BACKEND_URL, OPENAI_CHAT_URL, OPENAI_RESPONSES_URL},
    hooks::{NullHooks, WebhookHooks},
    mcp,
};

#[derive(Parser, Debug)]
#[command(
    name = "openai-proxy",
    about = "OpenAI-compatible proxy → Codex Responses API (uses ~/.codex/auth.json)"
)]
struct Args {
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: String,

    #[arg(long, env = "PORT", default_value_t = 8080)]
    port: u16,

    /// Path to Codex auth.json (default: ~/.codex/auth.json)
    #[arg(long, env = "CODEX_AUTH_PATH")]
    auth_path: Option<PathBuf>,

    /// Override the Codex backend URL
    #[arg(long, env = "CODEX_BACKEND_URL")]
    backend_url: Option<String>,

    /// Wire API format for API key users: "responses" (default) or "chat"
    #[arg(long, env = "CODEX_WIRE_API", default_value = "responses")]
    wire_api: String,

    /// Run as an MCP stdio server instead of the HTTP proxy
    #[arg(long)]
    mcp_stdio: bool,

    /// Also start an MCP Streamable HTTP server on this port
    #[arg(long, env = "MCP_HTTP_PORT")]
    mcp_http_port: Option<u16>,

    /// Expose the A2A Agent Card at GET /.well-known/agent.json
    #[arg(long)]
    a2a: bool,

    /// Path to hooks.toml config file for webhook event delivery
    #[arg(long, env = "PROXY_HOOKS_CONFIG")]
    hooks_config: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("openai_proxy=info".parse()?))
        .init();

    let args = Args::parse();

    let auth_path = args
        .auth_path
        .or_else(|| std::env::var("CODEX_AUTH_PATH").ok().map(PathBuf::from))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("cannot determine home directory")
                .join(".codex")
                .join("auth.json")
        });

    let auth = CodexAuth::load(&auth_path).unwrap_or_else(|e| {
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            tracing::warn!(
                path = %auth_path.display(),
                "auth.json not found ({e}); falling back to OPENAI_API_KEY"
            );
            CodexAuth {
                access_token: None,
                account_id: None,
                api_key: Some(key),
            }
        } else {
            panic!(
                "Cannot load {}: {e}\nRun `codex login` first, or set OPENAI_API_KEY.",
                auth_path.display()
            );
        }
    });

    let (backend_url, backend_profile) = if let Some(url) = args.backend_url {
        // Manual override: infer profile from URL
        let profile = if url.contains("chatgpt.com") {
            BackendProfile::ChatGptCodex
        } else if url.contains("/v1/chat/completions") || args.wire_api.eq_ignore_ascii_case("chat") {
            BackendProfile::OpenAiChatCompletions
        } else {
            BackendProfile::OpenAiResponses
        };
        (url, profile)
    } else if auth.access_token.is_some() {
        (CODEX_BACKEND_URL.to_string(), BackendProfile::ChatGptCodex)
    } else if args.wire_api.eq_ignore_ascii_case("chat") {
        (OPENAI_CHAT_URL.to_string(), BackendProfile::OpenAiChatCompletions)
    } else {
        (OPENAI_RESPONSES_URL.to_string(), BackendProfile::OpenAiResponses)
    };

    tracing::info!(backend = %backend_url, profile = %backend_profile.name(), "backend selected");

    let default_model = std::env::var("CODEX_DEFAULT_MODEL").ok();
    if let Some(ref m) = default_model {
        tracing::info!(model = %m, "default model override active");
    }

    // Load hooks if a config path is provided (via --hooks-config or PROXY_HOOKS_CONFIG).
    let hooks: Arc<dyn openai_proxy_lib::hooks::ProxyHooks + Send + Sync> =
        if let Some(ref path) = args.hooks_config {
            match WebhookHooks::from_config_file(path) {
                Ok(wh) => {
                    tracing::info!(path = %path, "webhook hooks loaded");
                    Arc::new(wh)
                }
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "failed to load hooks config; using NullHooks");
                    Arc::new(NullHooks)
                }
            }
        } else {
            Arc::new(NullHooks)
        };

    let state = AppState {
        auth,
        backend_url,
        backend_profile,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?,
        default_model,
        hooks,
    };

    if args.mcp_stdio {
        tracing::info!("starting in MCP stdio mode");
        return mcp::run_stdio(state).await;
    }

    if let Some(mcp_port) = args.mcp_http_port {
        let mcp_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = mcp::run_http(mcp_state, mcp_port).await {
                tracing::error!(error = %e, "MCP HTTP server error");
            }
        });
    }

    let app = build_app(state, args.a2a).layer(TraceLayer::new_for_http());

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("OpenAI proxy listening on http://{addr}");
    tracing::info!("endpoints: POST /v1/chat/completions  GET /v1/models  GET /health");
    tracing::info!("point opencode at: http://{addr}/v1");

    axum::serve(listener, app).await?;
    Ok(())
}
