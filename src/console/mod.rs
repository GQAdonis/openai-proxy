//! Same-origin console, owner-scoped REST conversations and local SDK supervision.
mod access;
mod admin;
mod conversations;
mod projection;
mod surfaces;
mod worker;

use crate::{AppState, config::ProxyConfig};
use access::{Access, Actor};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use fs2::FileExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs::File, path::PathBuf, sync::Arc};

#[derive(Default, Clone)]
pub struct RuntimeInfo {
    pub config: ProxyConfig,
    pub config_path: Option<PathBuf>,
    pub a2a: bool,
    pub mcp_http_port: Option<u16>,
    pub memory_scope: Option<String>,
    pub sources: std::collections::HashMap<String, String>,
}
#[derive(Debug)]
struct Error(StatusCode, &'static str);
impl Error {
    fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "authentication_required")
    }
    fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "forbidden")
    }
    fn bad() -> Self {
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_or_invalid_input",
        )
    }
    fn missing() -> Self {
        Self(StatusCode::NOT_FOUND, "not_found")
    }
    fn conflict() -> Self {
        Self(StatusCode::CONFLICT, "conflict")
    }
    fn storage() -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "console_storage_unavailable",
        )
    }
    fn unavailable() -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, "driver_unavailable")
    }
    fn busy() -> Self {
        Self(StatusCode::TOO_MANY_REQUESTS, "capacity_exceeded")
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":{"code":self.1,"message":self.1}})),
        )
            .into_response()
    }
}
type Result<T> = std::result::Result<T, Error>;
pub(super) type Console = Arc<Server>;
pub(super) struct Server {
    app: AppState,
    access: Access,
    root: PathBuf,
    turns: conversations::Registry,
    models: worker::ModelCache,
    settings: admin::Settings,
    provision: crate::cli::setup::provision::Provisioner,
    _lease: File,
}
impl Server {
    fn open(app: AppState) -> anyhow::Result<Self> {
        let root = std::env::var_os("PROXY_CONSOLE_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::config::data_dir().join("console"));
        access::private_dir(&root)?;
        let lease = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("service.lock"))?;
        lease.try_lock_exclusive()?;
        let access = Access::load(&app, &root)?;
        let turns = conversations::Registry::open(&root)?;
        let settings = admin::Settings::load(&root)?;
        let project = std::env::var_os("PROXY_CONSOLE_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?);
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("service home unavailable"))?;
        let provision = crate::cli::setup::provision::Provisioner::new(
            project,
            home,
            app.runtime.config.server.port,
            app.runtime.mcp_http_port.unwrap_or(8081),
        );
        Ok(Self {
            app,
            access,
            root,
            turns,
            models: Default::default(),
            settings,
            provision,
            _lease: lease,
        })
    }
    fn actor(&self, h: &HeaderMap, method: Method) -> Result<Actor> {
        self.access.authenticate(h, &method).map(|(a, _)| a)
    }
}

/// Reuse the console's native catalogue for the credential selected by the HTTP router.
#[derive(Clone)]
pub struct NativeModelDiscovery {
    server: Console,
    actor: Actor,
}
impl NativeModelDiscovery {
    pub(crate) async fn models(&self, state: &AppState) -> Option<Vec<Value>> {
        if state.backend_profile != crate::codex::BackendProfile::ChatGptCodex
            || state.credentials.codex_home().is_none()
        {
            return None;
        }
        let models = worker::catalog(&self.server, &self.actor).await.ok()?;
        let current = state
            .credentials
            .observed_snapshot(Some(state.owner_epoch))
            .ok()?;
        (current.generation == state.generation).then_some(models)
    }
}

pub fn router_with_legacy(state: AppState, legacy: Router, enable_a2a: bool) -> Router {
    let shared = std::env::var_os("PROXY_CONSOLE_PRINCIPALS").is_some();
    let unavailable = || async { Error::storage() };
    let server = match Server::open(state.clone()) {
        Ok(v) => Arc::new(v),
        Err(e) => {
            tracing::warn!(error=%e,"Console initialization failed");
            let mut r = Router::new()
                .route("/api/v1/{*path}", axum::routing::any(unavailable))
                .merge(crate::web_assets::router());
            if shared {
                r = r.fallback(unavailable);
            } else {
                r = r.merge(legacy);
            }
            return r;
        }
    };
    let weak = Arc::downgrade(&server);
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            let mut timer = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                timer.tick().await;
                let Some(s) = weak.upgrade() else { break };
                s.access.refresh_pairing();
            }
        });
    }
    let guarded = if shared {
        let s = server.clone();
        crate::direct_routes(enable_a2a).layer(middleware::from_fn(
            move |mut req: Request<Body>, next: Next| {
                let s = s.clone();
                async move {
                    let public = req.uri().path() == "/health"
                        || (enable_a2a && req.uri().path() == "/.well-known/agent.json");
                    let mut state = s.app.clone();
                    if !public {
                        let actor = match s.actor(req.headers(), req.method().clone()) {
                            Ok(v) => v,
                            Err(e) => return e.into_response(),
                        };
                        state.credentials = actor.credentials.clone();
                        Arc::make_mut(&mut state.runtime).memory_scope =
                            Some(format!("actor:{}", access::digest(actor.id.as_bytes())));
                        req.extensions_mut().insert(NativeModelDiscovery {
                            server: s.clone(),
                            actor,
                        });
                    }
                    req.extensions_mut().insert(state);
                    next.run(req).await
                }
            },
        ))
    } else if let Some(actor) = server.access.local_actor() {
        legacy.layer(Extension(NativeModelDiscovery {
            server: server.clone(),
            actor,
        }))
    } else {
        legacy
    };
    let api = Router::new()
        .route("/api/v1/session", get(session).post(pair).delete(logout))
        .route("/api/v1/me/connection", get(admin::connection))
        .route(
            "/api/v1/me/connection/{action}",
            post(admin::identity_action),
        )
        .route(
            "/api/v1/admin/settings",
            get(admin::settings).patch(admin::save),
        )
        .route("/api/v1/admin/setup/{op}", post(admin::setup))
        .route("/api/v1/me/setup/export", post(admin::visitor_export))
        .route("/api/v1/admin/config/{op}", post(admin::configure))
        .route("/api/v1/conversations", post(conversations::create))
        .route("/api/v1/conversations/{id}", get(conversations::get))
        .route(
            "/api/v1/conversations/{id}/turns",
            post(conversations::start),
        )
        .route(
            "/api/v1/conversations/{id}/turns/{turn}",
            get(conversations::turn),
        )
        .route(
            "/api/v1/conversations/{id}/turns/{turn}/events",
            get(conversations::events),
        )
        .route(
            "/api/v1/conversations/{id}/turns/{turn}/cancel",
            post(conversations::cancel),
        )
        .route(
            "/api/v1/conversations/{id}/turns/{turn}/permissions/{request}",
            post(conversations::permission),
        )
        .route("/api/v1/ag-ui/stream", post(projection::stream))
        .route("/api/v1/drivers/codex/v1/models", get(projection::models))
        .route(
            "/api/v1/drivers/codex/v1/chat/completions",
            post(projection::completion),
        )
        .route("/api/v1/drivers/codex/delegate", post(projection::delegate))
        .layer(DefaultBodyLimit::max(384 * 1024))
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                let mut resp = next.run(req).await;
                resp.headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                resp.headers_mut().insert(
                    "x-content-type-options",
                    HeaderValue::from_static("nosniff"),
                );
                resp
            },
        ))
        .with_state(server);
    guarded.merge(api).merge(crate::web_assets::router())
}
fn session_info(s: &Server, a: &Actor, csrf: Option<String>) -> Value {
    json!({"role":if a.admin{"admin"}else{"chat"},"mode":if s.access.shared{"shared"}else{"local"},"workspace":"default","csrfToken":csrf})
}
#[derive(Deserialize)]
struct Pair {
    token: String,
}
async fn pair(State(s): State<Console>, h: HeaderMap, Json(body): Json<Pair>) -> Result<Response> {
    let (a, cookie, csrf) = s.access.login(&h, &body.token)?;
    let mut resp = Json(session_info(&s, &a, Some(csrf))).into_response();
    let secure = if s.access.secure_cookie() {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "oproxy_session={cookie}; Path=/api/v1/; HttpOnly; SameSite=Strict; Max-Age=28800{secure}"
    );
    resp.headers_mut().insert(
        header::SET_COOKIE,
        cookie.parse().map_err(|_| Error::storage())?,
    );
    Ok(resp)
}
async fn session(State(s): State<Console>, h: HeaderMap) -> Result<Json<Value>> {
    let (a, csrf) = s.access.authenticate(&h, &Method::GET)?;
    Ok(Json(session_info(&s, &a, csrf)))
}
async fn logout(State(s): State<Console>, h: HeaderMap) -> Result<Response> {
    s.access.logout(&h)?;
    Ok((
        [(
            header::SET_COOKIE,
            "oproxy_session=; Path=/api/v1/; HttpOnly; SameSite=Strict; Max-Age=0",
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}
