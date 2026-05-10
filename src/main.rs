use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt};

use openai_proxy_lib::{
    AppState, build_app,
    acp,
    cli::{
        Command, SetupAction, SkillsAction, ConfigAction,
        setup::{ServeArgs, setup_opencode, setup_mcp, setup_config},
        skills::{skills_list, skills_validate, skills_test},
        config::{config_show, config_path},
    },
    codex::{BackendProfile, CodexAuth, CODEX_BACKEND_URL, OPENAI_CHAT_URL, OPENAI_RESPONSES_URL},
    config::{ProxyConfig, expand_tilde},
    hooks::{NullHooks, WebhookHooks},
    mcp,
    mcp_client::load_mcp_tools,
    skills::load_skills,
};

#[derive(Parser, Debug)]
#[command(
    name = "openai-proxy",
    about = "OpenAI-compatible proxy → Codex Responses API (uses ~/.codex/auth.json)",
    subcommand_negates_reqs = true
)]
struct Cli {
    /// Path to config.toml (default: $XDG_CONFIG_HOME/oproxy/config.toml)
    #[arg(long, env = "OPROXY_CONFIG", global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,

    /// Serve flags when no subcommand is given (backward compat)
    #[command(flatten)]
    serve: ServeArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("openai_proxy=info".parse()?))
        .init();

    let cli = Cli::parse();

    // Load config file first; env vars already applied by apply_env().
    let mut cfg = ProxyConfig::load(cli.config.as_deref()).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "config load failed; using defaults");
        ProxyConfig::default()
    });
    cfg.apply_env();

    // Dispatch non-serve subcommands without starting the server.
    match cli.command {
        Some(Command::Setup { action }) => {
            match action {
                SetupAction::Opencode(args) => setup_opencode(&args, None),
                SetupAction::Mcp(args) => setup_mcp(&args),
                SetupAction::Config => setup_config(),
            }
            return Ok(());
        }
        Some(Command::Skills { action }) => {
            match action {
                SkillsAction::List(args) => skills_list(&args, &cfg.skills.dirs),
                SkillsAction::Validate(args) => skills_validate(&args),
                SkillsAction::Test(args) => skills_test(&args, &cfg.skills.dirs),
            }
            return Ok(());
        }
        Some(Command::Config { action }) => {
            match action {
                ConfigAction::Show => config_show(&cfg),
                ConfigAction::Path => config_path(),
            }
            return Ok(());
        }
        Some(Command::Serve(serve)) => run_server(serve, cfg).await,
        None => run_server(cli.serve, cfg).await,
    }
}

async fn run_server(serve: ServeArgs, cfg: ProxyConfig) -> anyhow::Result<()> {
    let host = serve.host.unwrap_or(cfg.server.host.clone());
    let port = serve.port.unwrap_or(cfg.server.port);
    let wire_api = serve.wire_api.unwrap_or(cfg.backend.wire_api.clone());

    let auth_path = serve.auth_path.unwrap_or_else(|| {
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
            CodexAuth { access_token: None, account_id: None, api_key: Some(key) }
        } else {
            panic!(
                "Cannot load {}: {e}\nRun `codex login` first, or set OPENAI_API_KEY.",
                auth_path.display()
            );
        }
    });

    let (backend_url, backend_profile) = if let Some(url) = serve.backend_url {
        let profile = if url.contains("chatgpt.com") {
            BackendProfile::ChatGptCodex
        } else if url.contains("/v1/chat/completions") || wire_api.eq_ignore_ascii_case("chat") {
            BackendProfile::OpenAiChatCompletions
        } else {
            BackendProfile::OpenAiResponses
        };
        (url, profile)
    } else if auth.access_token.is_some() {
        (CODEX_BACKEND_URL.to_string(), BackendProfile::ChatGptCodex)
    } else if wire_api.eq_ignore_ascii_case("chat") {
        (OPENAI_CHAT_URL.to_string(), BackendProfile::OpenAiChatCompletions)
    } else {
        (OPENAI_RESPONSES_URL.to_string(), BackendProfile::OpenAiResponses)
    };

    tracing::info!(backend = %backend_url, profile = %backend_profile.name(), "backend selected");

    let default_model = std::env::var("CODEX_DEFAULT_MODEL").ok();
    if let Some(ref m) = default_model {
        tracing::info!(model = %m, "default model override active");
    }

    let hooks_config = serve.hooks_config.or(cfg.hooks.config_path.clone());
    let hooks: Arc<dyn openai_proxy_lib::hooks::ProxyHooks + Send + Sync> =
        if let Some(ref path) = hooks_config {
            match WebhookHooks::from_config_file(path) {
                Ok(wh) => { tracing::info!(path = %path, "webhook hooks loaded"); Arc::new(wh) }
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "failed to load hooks config; using NullHooks");
                    Arc::new(NullHooks)
                }
            }
        } else {
            Arc::new(NullHooks)
        };

    let bind_addr = format!("{host}:{port}");

    let skills = if !cfg.skills.dirs.is_empty() {
        let dirs: Vec<PathBuf> = cfg.skills.dirs.iter().map(|d| expand_tilde(d)).collect();
        let loaded = load_skills(&dirs);
        tracing::info!(count = loaded.len(), "skills loaded");
        loaded
    } else {
        openai_proxy_lib::skills::SkillIndex::build(vec![])
    };

    // Load MCP tool schemas if configured.
    let mcp_tools = if let Some(ref mcp_path_raw) = cfg.mcp.config_path {
        let mcp_path = expand_tilde(mcp_path_raw);
        let tools = load_mcp_tools(&mcp_path);
        if !tools.is_empty() {
            tracing::info!(count = tools.len(), "MCP tools loaded");
        }
        tools
    } else {
        Vec::new()
    };

    let enable_a2a = serve.a2a || cfg.modes.a2a;

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    // Initialize memory store when feature is compiled in and enabled in config.
    #[cfg(feature = "memory")]
    let (memory_backend, memory_store) = if cfg.memory.enabled {
        let db_path = if cfg.memory.db_path.is_empty() {
            openai_proxy_lib::config::data_dir().join("memory.db")
        } else {
            expand_tilde(&cfg.memory.db_path)
        };
        let api_key = auth.api_key.clone();
        match openai_proxy_lib::memory::MemoryStore::open(
            &db_path,
            http_client.clone(),
            cfg.memory.embedding_model.clone(),
            api_key,
        ).await {
            Ok(store) => {
                tracing::info!(path = %db_path.display(), "memory store opened");
                let backend: openai_proxy_lib::memory::DynMemory = store.clone();
                (backend, Some(store))
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to open memory store; continuing without memory");
                (openai_proxy_lib::memory::noop(), None)
            }
        }
    } else {
        (openai_proxy_lib::memory::noop(), None)
    };

    #[cfg(not(feature = "memory"))]
    let memory_backend = openai_proxy_lib::memory::noop();

    let state = AppState {
        auth,
        backend_url,
        backend_profile,
        http_client,
        default_model,
        hooks,
        bind_addr: bind_addr.clone(),
        skills: Arc::new(skills),
        mcp_tools: Arc::new(mcp_tools),
        memory: memory_backend,
        #[cfg(feature = "memory")]
        memory_store,
    };

    if serve.mcp_stdio {
        tracing::info!("starting in MCP stdio mode");
        return mcp::run_stdio(state).await;
    }

    if serve.acp_stdio {
        tracing::info!("starting in ACP stdio mode");
        return acp::run_acp_server(state).await;
    }

    if let Some(mcp_port) = serve.mcp_http_port {
        let mcp_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = mcp::run_http(mcp_state, mcp_port).await {
                tracing::error!(error = %e, "MCP HTTP server error");
            }
        });
    }

    let app = build_app(state, enable_a2a).layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("OpenAI proxy listening on http://{bind_addr}");
    tracing::info!("endpoints: POST /v1/chat/completions  GET /v1/models  GET /health  POST /ag-ui/stream");
    tracing::info!("point opencode at: http://{bind_addr}/v1");

    axum::serve(listener, app).await?;
    Ok(())
}
