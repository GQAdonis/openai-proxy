use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt};

use openai_proxy_lib::{
    AppState, acp,
    auth::CredentialProvider,
    build_app,
    cli::{
        Command, ConfigAction, SetupAction, SkillsAction,
        config::{config_path, config_show},
        setup::{ServeArgs, setup_acp, setup_config, setup_mcp, setup_opencode},
        skills::{skills_list, skills_test, skills_validate},
    },
    codex::{BackendProfile, CodexAuth},
    config::{ProxyConfig, expand_tilde},
    console::RuntimeInfo,
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
        .with_writer(std::io::stderr)
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
                SetupAction::Acp(args) => setup_acp(&args),
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
        Some(Command::Serve(serve)) => run_server(serve, cfg, cli.config).await,
        None => run_server(cli.serve, cfg, cli.config).await,
    }
}

async fn run_server(
    serve: ServeArgs,
    mut cfg: ProxyConfig,
    config_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    let sources = openai_proxy_lib::config::setting_sources(config_path.as_deref());
    let host = serve.host.unwrap_or(cfg.server.host.clone());
    let port = serve.port.unwrap_or(cfg.server.port);
    let wire_api = serve.wire_api.unwrap_or(cfg.backend.wire_api.clone());

    let shared = std::env::var_os("PROXY_CONSOLE_PRINCIPALS").is_some();
    if shared && (serve.acp_stdio || serve.mcp_stdio || serve.mcp_http_port.is_some()) {
        anyhow::bail!("shared mode requires an authenticated, owner-bound transport");
    }
    let credentials = if shared {
        CredentialProvider::disconnected()
    } else {
        CredentialProvider::process(serve.auth_path, &wire_api, serve.backend_url)?
    };
    cfg.server.host = host.clone();
    cfg.server.port = port;
    cfg.backend.wire_api = wire_api;

    let default_model = std::env::var("CODEX_DEFAULT_MODEL").ok();
    if let Some(ref m) = default_model {
        tracing::info!(model = %m, "default model override active");
    }

    let hooks_config = serve.hooks_config.or(cfg.hooks.config_path.clone());
    cfg.hooks.config_path = hooks_config.clone();
    let hooks: Arc<dyn openai_proxy_lib::hooks::ProxyHooks + Send + Sync> = {
        let configured = if let Some(ref path) = hooks_config {
            Some(WebhookHooks::from_config_file(path))
        } else {
            cfg.hooks
                .destination
                .as_deref()
                .map(WebhookHooks::from_trusted_destination)
        };
        match configured {
            Some(Ok(hooks)) => Arc::new(hooks),
            Some(Err(_)) => {
                tracing::warn!("webhook configuration unavailable");
                Arc::new(NullHooks)
            }
            None => Arc::new(NullHooks),
        }
    };

    let hooks: Arc<dyn openai_proxy_lib::hooks::ProxyHooks + Send + Sync> = if shared {
        Arc::new(openai_proxy_lib::hooks::SharedHooks(hooks))
    } else {
        hooks
    };

    anyhow::ensure!(
        !(serve.mcp_stdio && serve.acp_stdio),
        "select one stdio transport per process"
    );
    anyhow::ensure!(
        serve.mcp_http_port.is_none() || (!serve.mcp_stdio && !serve.acp_stdio),
        "MCP HTTP requires HTTP server mode"
    );
    let listener = if serve.mcp_stdio || serve.acp_stdio {
        None
    } else {
        Some(tokio::net::TcpListener::bind((host.as_str(), port)).await?)
    };
    let mcp_listener = if let Some(port) = serve.mcp_http_port {
        Some(tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?)
    } else {
        None
    };
    let mcp_http_port = mcp_listener
        .as_ref()
        .map(|l| l.local_addr())
        .transpose()?
        .map(|addr| addr.port());
    let bind_addr = if let Some(l) = &listener {
        let addr = l.local_addr()?;
        cfg.server.port = addr.port();
        addr.to_string()
    } else {
        format!("{host}:{port}")
    };

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

    cfg.modes.a2a = enable_a2a;

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        // Keep pooled connections to the upstream provider short-lived so a
        // connection half-closed by the peer (or an intermediate proxy) is
        // less likely to be reused after it's gone stale.
        .redirect(reqwest::redirect::Policy::none())
        .pool_idle_timeout(std::time::Duration::from_secs(50))
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .build()?;

    // Initialize memory store when feature is compiled in and enabled in config.
    #[cfg(feature = "memory")]
    let (memory_backend, memory_store) = if cfg.memory.enabled {
        let db_path = if cfg.memory.db_path.is_empty() {
            openai_proxy_lib::config::data_dir().join("memory.db")
        } else {
            expand_tilde(&cfg.memory.db_path)
        };
        let api_key = if shared {
            None
        } else {
            std::env::var("PROXY_EMBEDDINGS_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        };
        match openai_proxy_lib::memory::MemoryStore::open(
            &db_path,
            http_client.clone(),
            cfg.memory.embedding_model.clone(),
            api_key,
        )
        .await
        {
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
        credentials,
        runtime: Arc::new(RuntimeInfo {
            config: cfg.clone(),
            sources,
            config_path,
            a2a: enable_a2a,
            mcp_http_port,
            ..Default::default()
        }),
        generation: 0,
        owner_epoch: 0,
        auth: CodexAuth {
            api_key: None,
            account_id: None,
            access_token: None,
        },
        backend_url: String::new(),
        backend_profile: BackendProfile::ChatGptCodex,
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

    if let Some(mcp_listener) = mcp_listener {
        let mcp_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = mcp::run_http_on(mcp_state, mcp_listener).await {
                tracing::error!(error = %e, "MCP HTTP server error");
            }
        });
    }

    let app = build_app(state, enable_a2a).layer(TraceLayer::new_for_http());

    let listener = listener.ok_or_else(|| anyhow::anyhow!("HTTP listener unavailable"))?;
    tracing::info!("OpenAI proxy listening on http://{bind_addr}");
    tracing::info!(
        "endpoints: POST /v1/chat/completions  GET /v1/models  GET /health  POST /ag-ui/stream"
    );
    tracing::info!("point opencode at: http://{bind_addr}/v1");

    axum::serve(listener, app).await?;
    Ok(())
}
