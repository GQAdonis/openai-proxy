mod configure;
use super::{
    Console, Error, Result, Server,
    access::{self, Actor},
    worker,
};
use crate::cli::setup::provision::{Provisioner, SetupRequest, Transport};
use axum::{
    Json,
    extract::{OriginalUri, Path as RoutePath, State},
    http::{HeaderMap, Method},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Prefs {
    pub default_model: Option<String>,
    pub default_driver: String,
}
impl Default for Prefs {
    fn default() -> Self {
        Self {
            default_model: None,
            default_driver: "codex".into(),
        }
    }
}
pub(super) struct Settings {
    prefs: Mutex<Prefs>,
    path: PathBuf,
    changes: configure::Changes,
}
impl Settings {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let path = root.join("defaults.json");
        let prefs = match std::fs::read(&path) {
            Ok(data) => serde_json::from_slice(&data)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Prefs::default(),
            Err(e) => return Err(e.into()),
        };
        anyhow::ensure!(
            matches!(prefs.default_driver.as_str(), "codex" | "opencode")
                && prefs
                    .default_model
                    .as_deref()
                    .is_none_or(worker::valid_model),
            "invalid saved defaults"
        );
        Ok(Self {
            prefs: Mutex::new(prefs),
            path,
            changes: Default::default(),
        })
    }
    pub fn get(&self) -> Result<Prefs> {
        self.prefs
            .lock()
            .map(|v| v.clone())
            .map_err(|_| Error::storage())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}

fn operator(s: &Server, h: &HeaderMap, method: Method) -> Result<Actor> {
    let a = s.actor(h, method)?;
    if !a.admin {
        return Err(Error::forbidden());
    }
    Ok(a)
}
fn clean_uri(uri: &OriginalUri) -> Result<()> {
    if uri.0.query().is_some() {
        return Err(Error::bad());
    }
    Ok(())
}

async fn drivers(actor: &Actor) -> Vec<Value> {
    let mut rows = Vec::new();
    for driver in ["codex", "opencode"] {
        let (available, reason) = worker::availability(actor, driver).await;
        let controls = if driver == "codex" {
            vec!["stream", "stop", "resume"]
        } else {
            vec!["stream", "stop", "resume", "permissions"]
        };
        rows.push(json!({"id":driver,"available":available,"reason":reason,"controls":if available{controls}else{vec![]},"retrySafe":false}));
    }
    rows
}
async fn personal(s: &Console, actor: &Actor) -> Result<Value> {
    let status = actor.credentials.status().await;
    let profile = actor.credentials.observed_profile().await;
    let mut catalogue_status = "configured profile";
    let mut models: Vec<String> = if worker::availability(actor, "codex").await.0 {
        match worker::catalog(s, actor).await {
            Ok(rows) => {
                catalogue_status = "native catalogue";
                rows.iter()
                    .filter_map(|v| v["id"].as_str())
                    .filter(|id| worker::valid_model(id))
                    .map(str::to_owned)
                    .collect()
            }
            Err(_) => {
                catalogue_status = "native catalogue unavailable";
                Vec::new()
            }
        }
    } else {
        crate::model_catalog::CATALOG
            .iter()
            .filter(|e| profile.is_some_and(|p| e.supports(p)))
            .map(|e| e.model_id.to_string())
            .collect()
    };
    if let Some(p) = actor.provider.as_ref()
        && let Some(id) = p["model"].as_str().filter(|s| worker::valid_model(s))
        && !models.iter().any(|s| s == id)
    {
        models.push(id.into());
    }
    let prefs = s.settings.get()?;
    Ok(
        json!({"status":status.state,"mode":status.mode,"source":status.source,"refreshRuntime":status.refresh_runtime,"generation":status.generation,"lastReconciled":status.last_reconciled,
            "lastRefresh":status.last_renewed.map(|v|v.to_string()),"retryAfterSeconds":status.retry_after_seconds,
            "capabilities":{"reload":true,"refresh":status.can_refresh},"drivers":drivers(actor).await,"models":models,"catalogueStatus":catalogue_status,"workspace":"default",
            "defaultDriver":prefs.default_driver,"defaultModel":prefs.default_model.or_else(||s.app.default_model.clone()).unwrap_or_else(||"gpt-5.5".into())
        }),
    )
}
pub(super) async fn connection(
    State(s): State<Console>,
    h: HeaderMap,
    uri: OriginalUri,
) -> Result<Json<Value>> {
    let actor = s.actor(&h, Method::GET)?;
    clean_uri(&uri)?;
    Ok(Json(personal(&s, &actor).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Action {
    generation: Option<u64>,
}
pub(super) async fn identity_action(
    State(s): State<Console>,
    h: HeaderMap,
    uri: OriginalUri,
    RoutePath(action): RoutePath<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>> {
    let actor = s.actor(&h, Method::POST)?;
    clean_uri(&uri)?;
    let req = if body.is_empty() {
        Action { generation: None }
    } else {
        serde_json::from_slice(&body).map_err(|_| Error::bad())?
    };
    match action.as_str() {
        "reload" => {
            actor.credentials.reconcile_now().await;
        }
        "refresh" => {
            let expected = req.generation.ok_or_else(Error::bad)?;
            let before = actor.credentials.status().await;
            if before.generation != expected {
                return Err(Error::conflict());
            }
            if !before.can_refresh {
                return Err(Error::unavailable());
            }
            actor.credentials.refresh(expected).await;
        }
        _ => return Err(Error::missing()),
    }
    Ok(Json(personal(&s, &actor).await?))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Defaults {
    default_model: Option<String>,
    default_driver: Option<String>,
}
pub(super) async fn save(
    State(s): State<Console>,
    h: HeaderMap,
    Json(edit): Json<Defaults>,
) -> Result<Json<Value>> {
    let actor = operator(&s, &h, Method::PATCH)?;
    if edit.default_model.is_none() && edit.default_driver.is_none() {
        return Err(Error::bad());
    }
    if edit
        .default_model
        .as_deref()
        .is_some_and(|m| !worker::valid_model(m))
    {
        return Err(Error::bad());
    }
    if let Some(d) = edit.default_driver.as_deref()
        && (!matches!(d, "codex" | "opencode") || !worker::availability(&actor, d).await.0)
    {
        return Err(Error::unavailable());
    }
    let mut prefs = s.settings.prefs.lock().map_err(|_| Error::storage())?;
    let next = Prefs {
        default_model: edit.default_model.or_else(|| prefs.default_model.clone()),
        default_driver: edit
            .default_driver
            .unwrap_or_else(|| prefs.default_driver.clone()),
    };
    access::atomic_json(&s.settings.path, &next)?;
    *prefs = next;
    Ok(Json(
        json!({"status":"applied","activation":"new conversations"}),
    ))
}

pub(super) async fn configure(
    State(s): State<Console>,
    h: HeaderMap,
    RoutePath(op): RoutePath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let actor = operator(&s, &h, Method::POST)?;
    tokio::task::spawn_blocking(move || {
        s.settings.changes.dispatch(&s, &actor, &op, body).map(Json)
    })
    .await
    .map_err(|_| Error::storage())?
}

fn setup_request(mut body: Value) -> Result<SetupRequest> {
    if let Some(op) = body.as_object_mut().and_then(|obj| obj.remove("operation"))
        && op != "install"
    {
        return Err(Error::bad());
    }
    serde_json::from_value(body).map_err(|_| Error::bad())
}
pub(super) async fn visitor_export(
    State(s): State<Console>,
    h: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    s.actor(&h, Method::POST)?;
    Provisioner::portable_export(setup_request(body)?)
        .map(Json)
        .map_err(|_| Error::bad())
}
pub(super) async fn setup(
    State(s): State<Console>,
    h: HeaderMap,
    RoutePath(op): RoutePath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let a = operator(&s, &h, Method::POST)?;
    tokio::task::spawn_blocking(move||{
        let response=match op.as_str(){
            "preview"|"export"=>{
                let req=setup_request(body)?;
                if op=="export"{s.provision.export(req)}
                else if matches!(req.transport,Transport::Http) && s.app.runtime.mcp_http_port.is_none(){
                    return Ok(Json(json!({"status":"blocked","reason":"MCP HTTP listener is unavailable","activation":"service restart"})))
                }else{s.provision.preview(&a.id,req)}
            }
            "apply"|"undo"=>{let Id{id}=serde_json::from_value(body).map_err(|_|Error::bad())?;
                if op=="apply"{s.provision.apply(&a.id,&id)}else{s.provision.undo(&a.id,&id)}}
            _=>return Err(Error::missing()),
        };response.map(Json).map_err(|_|Error::conflict())
    }).await.map_err(|_|Error::storage())?
}

pub(super) async fn settings(State(s): State<Console>, h: HeaderMap) -> Result<Json<Value>> {
    let a = operator(&s, &h, Method::GET)?;
    Ok(Json(inventory(&s, &a).await?))
}

async fn inventory(s: &Console, a: &Actor) -> Result<Value> {
    let app = &s.app;
    let cfg = &app.runtime.config;
    let persisted = configure::file_config(s);
    let personal = personal(s, a).await?;
    let mode = if s.access.shared { "shared" } else { "local" };
    let safe_defaults = s.settings.get()?;
    let memory_status = if !cfg!(feature = "memory") {
        "build_disabled"
    } else if !cfg.memory.enabled {
        "disabled"
    } else if !app.memory.is_enabled() {
        "database_unavailable"
    } else {
        "ready"
    };
    let hook_events = app.hooks.configured_events();
    let hook_failure = app.hooks.last_failure();
    let hook_status = if hook_events.is_empty() {
        if cfg.hooks.config_path.is_some() || cfg.hooks.destination.is_some() {
            "no_active_destinations"
        } else {
            "unconfigured"
        }
    } else if hook_failure.is_some() {
        "delivery_failure_observed"
    } else {
        "configured"
    };
    let source = |key: &str| {
        app.runtime
            .sources
            .get(key)
            .map(String::as_str)
            .unwrap_or("default")
    };
    let field = |key: &str, effective: Value| {
        let configured = key
            .split_once('.')
            .and_then(|(section, k)| persisted.as_ref()?.get(section)?.get(k))
            .and_then(|v| serde_json::to_value(v).ok());
        let source = source(key);
        json!({"field":key,"effective":effective,"configured":configured,"origin":source,"overridden":matches!(source,"cli"|"environment"),
            "pendingRestart":!matches!(source,"cli"|"environment") && configured.as_ref().is_some_and(|v|v!=&effective)})
    };
    let edit = |key: &str, kind: &str| {
        if configure::mutable(s, key) {
            Some(json!({"field":key,"type":kind}))
        } else {
            None
        }
    };
    let mut areas = Vec::new();
    let mut area = |id: &str,
                    title: &str,
                    value: Value,
                    source: &str,
                    activation: &str,
                    availability: &str,
                    action: Option<Value>| {
        areas.push(json!({"id":id,"title":title,"value":value,"source":source,"scope":"service administrator",
            "activation":activation,"availability":availability,"action":action,"writable":action.is_some()}));
    };
    area(
        "deployment",
        "Deployment and access",
        json!({"mode":mode,"actorRole":"administrator","workspace":"default",
        "browserOrigin":std::env::var_os("PROXY_CONSOLE_ORIGIN").is_some(),"otherPrincipalsVisible":false}),
        "service access policy",
        "service restart",
        "ready",
        None,
    );
    area(
        "service",
        "Service, logs and configuration",
        json!({"host":field("server.host",json!(cfg.server.host)),
        "port":field("server.port",json!(cfg.server.port)),"version":env!("CARGO_PKG_VERSION"),"health":"/health",
        "config":app.runtime.config_path.clone().unwrap_or_else(||crate::config::config_dir().join("config.toml")),
        "logFilterSource":if std::env::var_os("RUST_LOG").is_some(){"environment"}else{"default"},"logOutput":"stderr"}),
        source("server.port"),
        "service restart",
        if persisted.is_some() {
            "ready"
        } else {
            "config_unreadable"
        },
        edit("server.port", "integer"),
    );
    area(
        "identity",
        "Identity and routing",
        json!({"state":personal["status"],"mode":personal["mode"],"source":personal["source"],
        "generation":personal["generation"],"lastRefresh":personal["lastRefresh"],"retryAfterSeconds":personal["retryAfterSeconds"],
        "wire":field("backend.wire_api",json!(cfg.backend.wire_api))}),
        source("backend.wire_api"),
        "login changes live; route policy on restart",
        "owner-scoped",
        edit("backend.wire_api", "select"),
    );
    area(
        "chat",
        "Chat and models",
        json!({"defaultDriver":safe_defaults.default_driver,"defaultModel":personal["defaultModel"],"models":personal["models"],
        "directChat":"/v1/chat/completions","modelDiscovery":"/v1/models","SDKControls":["stream","stop","resume"]}),
        if safe_defaults.default_model.is_some() {
            "console defaults"
        } else if app.default_model.is_some() {
            "environment"
        } else {
            "catalogue default"
        },
        "new conversations",
        "ready",
        Some(json!({"method":"PATCH","endpoint":"/api/v1/admin/settings"})),
    );
    area(
        "sdk",
        "SDK harnesses",
        json!({"drivers":personal["drivers"],"workspace":"default","codexSdk":"0.169.0","openCodeSdk":"2.3.0"}),
        "installed native workers and actor-approved providers",
        "per conversation",
        "see each driver",
        None,
    );
    area(
        "opencode",
        "OpenCode on this host",
        json!({"versionRange":">=2.3.0 <3.0.0 stable","certificationTarget":"2.3.0","provider":"openai-proxy",
        "modes":["native plugin","static V2 provider"],"scopes":["project","global"],"codexMarketplacePlugin":"separate installation","discovery":"version and effective sources checked at preview"}),
        "installed client and effective JSONC layers",
        "client reload",
        "requires versioned preview",
        Some(json!({"endpoint":"/api/v1/admin/setup/preview"})),
    );
    area(
        "machine",
        "Safe machine configuration",
        json!({"target":"service host","visitor":"downloadable descriptor","scope":["project","global"],"previewExpirySeconds":300,"preservesUnrelatedSettings":true}),
        "versioned setup writer",
        "client reload",
        "ready",
        Some(json!({"operations":["preview","apply","undo","export"]})),
    );
    area(
        "acp",
        "ACP clients",
        json!({"transport":"dedicated local stdio","clients":["Zed","JetBrains AI Assistant"],"launch":["openai-proxy","serve","--acp-stdio"],"stream":true,"resume":false,"permissions":false,"toolExecution":false,"cancel":false}),
        "installed ACP adapter",
        "client launch",
        if s.access.shared {
            "requires dedicated owner process"
        } else {
            "available on separate stdio process"
        },
        Some(json!({"operation":"client setup"})),
    );
    area(
        "mcp",
        "Proxy as MCP server",
        json!({"transports":["dedicated stdio","loopback Streamable HTTP"],"httpPort":app.runtime.mcp_http_port,"httpAuthorization":if std::env::var_os("PROXY_MCP_TOKEN").is_some(){"local bearer required"}else{"dedicated loopback"},"tools":["chat_completion","list_models","check_auth","set_model"],
        "setModel":"advisory","clients":["Codex CLI","Claude Code","OpenCode V2","Zed","VS Code/Copilot"]}),
        "runtime listener and MCP adapter",
        "client reload or service restart",
        if app.runtime.mcp_http_port.is_some() {
            "loopback HTTP listening"
        } else {
            "stdio launch available; HTTP disabled"
        },
        Some(json!({"operation":"client setup"})),
    );
    area(
        "tools",
        "Tool imports",
        json!({"schemaCount":app.mcp_tools.len(),"schemas":app.mcp_tools.iter().map(|t|&t.function.name).collect::<Vec<_>>(),"executor":false,
        "nativeSdkTools":"subject to explicit native policy"}),
        source("mcp.config_path"),
        "service restart",
        if app.mcp_tools.is_empty() {
            if cfg.mcp.config_path.is_some() {
                "empty or unavailable source"
            } else {
                "unconfigured"
            }
        } else {
            "schemas loaded"
        },
        None,
    );
    area(
        "agui",
        "AG-UI and A2UI",
        json!({"textEndpoint":"/ag-ui/stream","structuredEndpoint":"/api/v1/ag-ui/stream","surfaceVersion":"1.0","scope":"authenticated conversation",
        "state":"registered catalogue and schema validation"}),
        "embedded protocol adapters",
        "build time",
        "available",
        None,
    );
    area(
        "a2a",
        "A2A discovery",
        json!({"enabled":field("modes.a2a",json!(app.runtime.a2a)),"path":"/.well-known/agent.json",
        "advertisedUrl":crate::a2a::advertised(app),"skills":if app.runtime.a2a{vec!["chat_completion","list_models"]}else{vec![]}}),
        source("modes.a2a"),
        "service restart",
        if app.runtime.a2a {
            "enabled"
        } else {
            "disabled"
        },
        edit("modes.a2a", "boolean"),
    );
    area(
        "skills",
        "Skills",
        json!({"directories":field("skills.dirs",json!(cfg.skills.dirs)),"loaded":app.skills.len(),
        "maxInjected":field("skills.max_injected",json!(cfg.skills.max_injected)),"consumers":["direct HTTP","ACP prompts"]}),
        source("skills.max_injected"),
        "service restart",
        if app.skills.is_empty() {
            "no loaded skills"
        } else {
            "ready"
        },
        edit("skills.max_injected", "integer"),
    );
    let mut aliases = crate::hooks::WebhookHooks::trusted_destinations()
        .into_keys()
        .collect::<Vec<_>>();
    aliases.sort();
    let hook_edit = edit("hooks.destination", "select").map(|mut e| {
        e["choices"] = json!(aliases);
        e
    });
    area(
        "webhooks",
        "Webhooks",
        json!({"events":hook_events,"emitter":"direct HTTP chat","destinationAlias":field("hooks.destination",json!(cfg.hooks.destination)),
        "approvedAliases":aliases,"lastFailure":hook_failure}),
        source("hooks.config_path"),
        "service restart",
        hook_status,
        hook_edit,
    );
    area(
        "memory",
        "Memory and retrieval",
        json!({"compiled":cfg!(feature="memory"),"enabled":field("memory.enabled",json!(cfg.memory.enabled)),"database":if cfg.memory.db_path.is_empty(){crate::config::data_dir().join("memory.db")}else{crate::config::expand_tilde(&cfg.memory.db_path)},
        "embeddingModel":cfg.memory.embedding_model,"embeddings":if !s.access.shared && std::env::var("PROXY_EMBEDDINGS_API_KEY").is_ok_and(|s|!s.is_empty()){"separate API key, API billing"}else{"disabled; local document retrieval"},
        "scope":if s.access.shared{"assigned by authenticated principal"}else{"local owner"}}),
        source("memory.enabled"),
        if cfg!(feature = "memory") {
            "service restart"
        } else {
            "build with memory feature"
        },
        memory_status,
        edit("memory.enabled", "boolean"),
    );
    area(
        "packaging",
        "Packaging and process topology",
        json!({"version":env!("CARGO_PKG_VERSION"),"process":"HTTP server","console":"/ui/","mcpHttp":app.runtime.mcp_http_port.is_some(),
        "memoryCompiled":cfg!(feature="memory"),"sdkDependencies":personal["drivers"],"stdioOnly":"separate process without a console"}),
        "running service and embedded build",
        "build time or service restart",
        "ready",
        None,
    );
    Ok(json!({"areas":areas}))
}
