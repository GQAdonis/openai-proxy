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

pub mod provision;
use provision::{Integration, Provisioner, Scope, SetupRequest, Transport};

#[derive(Args, Debug)]
pub struct SetupOpencodeArgs {
    /// Configure the global OpenCode host.
    #[arg(long, conflicts_with = "project")]
    pub global: bool,
    /// Configure this checkout (default).
    #[arg(long)]
    pub project: bool,
    /// Register the V2 server plugin instead of a static provider.
    #[arg(long)]
    pub plugin: bool,
    #[arg(long, default_value_t = 8181)]
    pub port: u16,
    #[arg(long, alias = "preview")]
    pub dry_run: bool,
    /// Retained for CLI compatibility. File revision and ownership checks always apply.
    #[arg(long)]
    pub force: bool,
    /// Undo an applied setup using the ID on its receipt.
    #[arg(long)]
    pub undo: Option<String>,
}

#[derive(Args, Debug)]
pub struct SetupMcpArgs {
    #[arg(long)]
    pub opencode: bool,
    #[arg(long)]
    pub claude: bool,
    #[arg(long)]
    pub codex: bool,
    #[arg(long)]
    pub zed: bool,
    #[arg(long)]
    pub vscode: bool,
    #[arg(long, conflicts_with = "global")]
    pub project: bool,
    #[arg(long)]
    pub global: bool,
    /// Use the proxy stdio process (default transport for this command is HTTP).
    #[arg(long)]
    pub stdio: bool,
    #[arg(long, default_value_t = 8081)]
    pub port: u16,
    #[arg(long, alias = "preview")]
    pub dry_run: bool,
    #[arg(long)]
    pub undo: Option<String>,
}

#[derive(Args, Debug)]
pub struct SetupAcpArgs {
    #[arg(long)]
    pub zed: bool,
    #[arg(long)]
    pub jetbrains: bool,
    /// Emit a portable stdio launch descriptor.
    #[arg(long)]
    pub generic: bool,
    #[arg(long, conflicts_with = "global")]
    pub project: bool,
    #[arg(long)]
    pub global: bool,
    #[arg(long, alias = "preview")]
    pub dry_run: bool,
    #[arg(long)]
    pub undo: Option<String>,
}

fn local_service(port: u16, mcp_port: u16, base_url: Option<&str>) -> anyhow::Result<Provisioner> {
    let project = std::env::current_dir()?;
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory unavailable"))?;
    Provisioner::new(project, home, port, mcp_port).with_base_url(base_url)
}

fn run_setup(
    service: anyhow::Result<Provisioner>,
    integrations: &[Integration],
    scope: Scope,
    transport: Transport,
    preview_only: bool,
    undo: Option<&str>,
) {
    const ACTOR: &str = "local-cli-owner";
    let perform = || -> anyhow::Result<()> {
        let service = service?;
        if let Some(id) = undo {
            println!("{}", service.undo(ACTOR, id)?);
            return Ok(());
        }
        anyhow::ensure!(!integrations.is_empty(), "select at least one client");
        let mut blocked = false;
        for integration in integrations {
            let req = SetupRequest {
                integration: *integration,
                scope,
                transport,
            };
            let preview = service.preview(ACTOR, req)?;
            println!("{}", serde_json::to_string_pretty(&preview)?);
            if preview["status"] == "blocked" {
                blocked = true;
                continue;
            }
            if !preview_only && preview["status"] == "ready" {
                let id = preview["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing preview ID"))?;
                println!("{}", service.apply(ACTOR, id)?);
            }
        }
        anyhow::ensure!(
            !blocked,
            "one or more clients are unavailable for local setup"
        );
        Ok(())
    };
    if let Err(err) = perform() {
        eprintln!("setup failed: {err}");
        std::process::exit(1);
    }
}

pub fn setup_opencode(args: &SetupOpencodeArgs, base_url: Option<&str>) {
    let integration = if args.plugin {
        Integration::OpencodePlugin
    } else {
        Integration::OpencodeProvider
    };
    run_setup(
        local_service(args.port, 8081, base_url),
        &[integration],
        if args.global {
            Scope::Global
        } else {
            Scope::Project
        },
        Transport::Stdio,
        args.dry_run,
        args.undo.as_deref(),
    );
}
pub fn setup_mcp(args: &SetupMcpArgs) {
    let clients = [
        (args.opencode, Integration::OpencodeMcp),
        (args.claude, Integration::ClaudeMcp),
        (args.codex, Integration::CodexMcp),
        (args.zed, Integration::ZedMcp),
        (args.vscode, Integration::VscodeMcp),
    ];
    let selected = clients
        .into_iter()
        .filter_map(|(enabled, client)| enabled.then_some(client))
        .collect::<Vec<_>>();
    run_setup(
        local_service(8181, args.port, None),
        &selected,
        if args.project {
            Scope::Project
        } else {
            Scope::Global
        },
        if args.stdio {
            Transport::Stdio
        } else {
            Transport::Http
        },
        args.dry_run,
        args.undo.as_deref(),
    );
}
pub fn setup_acp(args: &SetupAcpArgs) {
    let clients = [
        (args.zed, Integration::ZedAcp),
        (args.jetbrains, Integration::JetbrainsAcp),
        (args.generic, Integration::GenericAcp),
    ];
    let selected = clients
        .into_iter()
        .filter_map(|(enabled, client)| enabled.then_some(client))
        .collect::<Vec<_>>();
    run_setup(
        local_service(8181, 8081, None),
        &selected,
        if args.project {
            Scope::Project
        } else {
            Scope::Global
        },
        Transport::Stdio,
        args.dry_run,
        args.undo.as_deref(),
    );
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
port = 8181

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
        Err(e) => {
            eprintln!("error writing config: {e}");
            std::process::exit(1);
        }
    }
}
