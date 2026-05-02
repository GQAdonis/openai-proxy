use std::path::PathBuf;

use clap::Args;

// ── Serve (default) ──────────────────────────────────────────────────────────

#[derive(Args, Debug, Default)]
pub struct ServeArgs {
    #[arg(long, env = "HOST")]
    pub host: Option<String>,

    #[arg(long, env = "PORT")]
    pub port: Option<u16>,

    /// Path to Codex auth.json (default: ~/.codex/auth.json)
    #[arg(long, env = "CODEX_AUTH_PATH")]
    pub auth_path: Option<PathBuf>,

    /// Override the Codex backend URL
    #[arg(long, env = "CODEX_BACKEND_URL")]
    pub backend_url: Option<String>,

    /// Wire API format for API key users: "responses" (default) or "chat"
    #[arg(long, env = "CODEX_WIRE_API")]
    pub wire_api: Option<String>,

    /// Run as an MCP stdio server instead of the HTTP proxy
    #[arg(long)]
    pub mcp_stdio: bool,

    /// Run as an ACP stdio server instead of the HTTP proxy
    #[arg(long)]
    pub acp_stdio: bool,

    /// Also start an MCP Streamable HTTP server on this port
    #[arg(long, env = "MCP_HTTP_PORT")]
    pub mcp_http_port: Option<u16>,

    /// Expose the A2A Agent Card at GET /.well-known/agent.json
    #[arg(long)]
    pub a2a: bool,

    /// Path to hooks.toml config file for webhook event delivery
    #[arg(long, env = "PROXY_HOOKS_CONFIG")]
    pub hooks_config: Option<String>,
}

// ── Setup opencode ───────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SetupOpencodeArgs {
    /// Write to the global opencode config (~/.config/opencode/opencode.json)
    #[arg(long)]
    pub global: bool,

    /// Proxy port to register in opencode config
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Print what would be written without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Overwrite existing provider entry if present
    #[arg(long)]
    pub force: bool,
}

pub fn setup_opencode(args: &SetupOpencodeArgs, base_url: Option<&str>) {
    let url = base_url
        .map(str::to_owned)
        .unwrap_or_else(|| format!("http://127.0.0.1:{}/v1", args.port));

    // Detect auth to choose models list.
    let auth_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".codex")
        .join("auth.json");
    let is_chatgpt_sub = auth_path.exists() && {
        std::fs::read_to_string(&auth_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("access_token").map(|t| !t.is_null()))
            .unwrap_or(false)
    };

    let (provider_name, models) = if is_chatgpt_sub {
        (
            "OpenAI Proxy (ChatGPT Subscription Plus/Pro)",
            serde_json::json!({
                "gpt-5.3-codex": { "name": "GPT-5.3 Codex", "limit": { "context": 400000, "output": 32768 } },
                "gpt-5.4":       { "name": "GPT-5.4",       "limit": { "context": 400000, "output": 32768 } },
                "gpt-5.5":       { "name": "GPT-5.5",       "limit": { "context": 400000, "output": 32768 } }
            }),
        )
    } else {
        (
            "OpenAI Proxy (API Key — Responses API)",
            serde_json::json!({
                "gpt-5.5":       { "name": "GPT-5.5",       "limit": { "context": 1000000, "output": 32768 } },
                "gpt-5.5-pro":   { "name": "GPT-5.5 Pro",   "limit": { "context": 1000000, "output": 32768 } },
                "gpt-5.4":       { "name": "GPT-5.4",       "limit": { "context": 200000,  "output": 16384 } },
                "gpt-5.3-codex": { "name": "GPT-5.3 Codex", "limit": { "context": 200000,  "output": 16384 } },
                "codex-mini":    { "name": "Codex Mini",     "limit": { "context": 96000,   "output": 8192  } }
            }),
        )
    };

    let default_model = if is_chatgpt_sub { "openai-proxy/gpt-5.5" } else { "openai-proxy/gpt-5.5" };

    let config_dir = if args.global {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("opencode")
    } else {
        PathBuf::from(".opencode")
    };
    let config_path = config_dir.join("opencode.json");

    let provider_entry = serde_json::json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": provider_name,
        "options": {
            "baseURL": url,
            "apiKey": "not-required"
        },
        "models": models
    });

    if args.dry_run {
        let preview = serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "provider": { "openai-proxy": provider_entry },
            "model": default_model
        });
        println!("# Would write to: {}", config_path.display());
        println!("{}", serde_json::to_string_pretty(&preview).unwrap());
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        eprintln!("error: cannot create {}: {e}", config_dir.display());
        std::process::exit(1);
    }

    let mut existing: serde_json::Value = if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({ "$schema": "https://opencode.ai/config.json" })
    };

    if !args.force {
        if let Some(providers) = existing.get("provider") {
            if providers.get("openai-proxy").is_some() {
                println!("openai-proxy provider already configured in {}. Use --force to overwrite.", config_path.display());
                return;
            }
        }
    }

    // Merge provider entry.
    let providers = existing
        .as_object_mut()
        .unwrap()
        .entry("provider")
        .or_insert(serde_json::json!({}));
    providers.as_object_mut().unwrap().insert("openai-proxy".to_string(), provider_entry);

    // Set default model if not already set.
    if existing.get("model").is_none() {
        existing.as_object_mut().unwrap().insert("model".to_string(), serde_json::Value::String(default_model.to_string()));
    }

    match serde_json::to_string_pretty(&existing) {
        Ok(json) => match std::fs::write(&config_path, json) {
            Ok(_) => {
                println!("opencode config written to {}", config_path.display());
                println!("Point opencode at this proxy: model = {default_model}");
            }
            Err(e) => { eprintln!("error writing {}: {e}", config_path.display()); std::process::exit(1); }
        },
        Err(e) => { eprintln!("error serializing config: {e}"); std::process::exit(1); }
    }
}

// ── Setup MCP ────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SetupMcpArgs {
    /// Add the proxy as an MCP server to opencode's config
    #[arg(long)]
    pub opencode: bool,

    /// Add the proxy as an MCP server to Claude Code's config (~/.claude.json)
    #[arg(long)]
    pub claude: bool,

    /// MCP server port to register
    #[arg(long, default_value_t = 8081)]
    pub port: u16,
}

pub fn setup_mcp(args: &SetupMcpArgs) {
    if !args.opencode && !args.claude {
        eprintln!("error: specify --opencode and/or --claude");
        std::process::exit(1);
    }

    let mcp_url = format!("http://127.0.0.1:{}/mcp", args.port);

    if args.opencode {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("opencode");
        let config_path = config_dir.join("opencode.json");

        if let Err(e) = std::fs::create_dir_all(&config_dir) {
            eprintln!("error: cannot create {}: {e}", config_dir.display());
            std::process::exit(1);
        }

        let mut existing: serde_json::Value = if config_path.exists() {
            let raw = std::fs::read_to_string(&config_path).unwrap_or_default();
            serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({ "$schema": "https://opencode.ai/config.json" })
        };

        let mcp_entry = serde_json::json!({
            "type": "remote",
            "url": mcp_url
        });

        let mcp_section = existing
            .as_object_mut()
            .unwrap()
            .entry("mcp")
            .or_insert(serde_json::json!({}));
        mcp_section.as_object_mut().unwrap().insert("openai-proxy".to_string(), mcp_entry);

        match serde_json::to_string_pretty(&existing) {
            Ok(json) => match std::fs::write(&config_path, json) {
                Ok(_) => println!("opencode MCP config written to {}", config_path.display()),
                Err(e) => eprintln!("error writing opencode config: {e}"),
            },
            Err(e) => eprintln!("error serializing opencode config: {e}"),
        }
    }

    if args.claude {
        let claude_config = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".claude.json");

        let mut existing: serde_json::Value = if claude_config.exists() {
            let raw = std::fs::read_to_string(&claude_config).unwrap_or_default();
            serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let mcp_entry = serde_json::json!({
            "type": "http",
            "url": mcp_url
        });

        let mcp_servers = existing
            .as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert(serde_json::json!({}));
        mcp_servers.as_object_mut().unwrap().insert("openai-proxy".to_string(), mcp_entry);

        match serde_json::to_string_pretty(&existing) {
            Ok(json) => match std::fs::write(&claude_config, json) {
                Ok(_) => println!("Claude Code MCP config written to {}", claude_config.display()),
                Err(e) => eprintln!("error writing claude config: {e}"),
            },
            Err(e) => eprintln!("error serializing claude config: {e}"),
        }
    }
}

// ── Setup config scaffold ────────────────────────────────────────────────────

pub fn setup_config() {
    let dir = crate::config::config_dir();
    let path = dir.join("config.toml");

    if path.exists() {
        println!("config already exists at {}", path.display());
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("error: cannot create {}: {e}", dir.display());
        std::process::exit(1);
    }

    let template = r#"[server]
host = "0.0.0.0"
port = 8080

[backend]
wire_api = "responses"

[skills]
dirs = []
max_injected = 3

[mcp]
# config_path = "~/.config/oproxy/mcp.toml"

[hooks]
# config_path = "~/.config/oproxy/hooks.toml"

[memory]
enabled = false
db_path = ""
embedding_model = "text-embedding-3-small"

[modes]
a2a = false
"#;

    match std::fs::write(&path, template) {
        Ok(_) => println!("config scaffolded at {}", path.display()),
        Err(e) => { eprintln!("error writing config: {e}"); std::process::exit(1); }
    }
}
