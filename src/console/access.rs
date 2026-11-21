use super::{Error, Result};
use crate::{AppState, auth::CredentialProvider};
use axum::http::{HeaderMap, Method};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Actor {
    pub id: String,
    pub admin: bool,
    pub workspace: PathBuf,
    pub credentials: CredentialProvider,
    pub provider: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Principal {
    id: String,
    role: String,
    token_sha256: String,
    workspace: PathBuf,
    codex_home: PathBuf,
    #[serde(default)]
    open_code_provider: Option<Value>,
}

struct Session {
    actor: usize,
    csrf: String,
    expires: Instant,
}
pub(super) struct Access {
    pub shared: bool,
    actors: Vec<(String, Actor)>,
    sessions: Mutex<HashMap<String, Session>>,
    origin: Option<String>,
    pairing: Option<Mutex<Pairing>>,
}

pub(super) fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub(super) fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub(super) fn private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return Err(std::io::Error::other("expected a private directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::other(
                "console directory must be owner-only",
            ));
        }
    }
    Ok(())
}
pub(super) fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    atomic_json_limited(path, value, u64::MAX)
}
pub(super) fn atomic_json_limited(
    path: &Path,
    value: &impl serde::Serialize,
    limit: u64,
) -> Result<()> {
    let parent = path.parent().ok_or_else(Error::storage)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::storage())?;
    serde_json::to_writer(tmp.as_file_mut(), value).map_err(|_| Error::storage())?;
    if tmp
        .as_file()
        .metadata()
        .map_err(|_| Error::storage())?
        .len()
        > limit
    {
        return Err(Error::busy());
    }
    tmp.as_file().sync_all().map_err(|_| Error::storage())?;
    tmp.persist(path).map_err(|_| Error::storage())?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::storage())?;
    Ok(())
}
struct Pairing {
    path: PathBuf,
    hash: String,
    expires: Instant,
}
impl Pairing {
    fn rotate(&mut self) -> std::io::Result<()> {
        let token = secret();
        let mut file = tempfile::NamedTempFile::new_in(
            self.path
                .parent()
                .ok_or_else(|| std::io::Error::other("pairing path"))?,
        )?;
        writeln!(file, "{token}")?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        self.hash = digest(token.as_bytes());
        self.expires = Instant::now() + Duration::from_secs(300);
        Ok(())
    }
}
fn service_token(root: &Path) -> anyhow::Result<String> {
    if let Ok(token) = std::env::var("PROXY_CONSOLE_TOKEN") {
        anyhow::ensure!(
            token.len() >= 32 && token.len() <= 512,
            "invalid configured console token"
        );
        return Ok(token);
    }
    let path = root.join("service-token");
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    writeln!(file, "{}", secret())?;
    file.as_file().sync_all()?;
    if let Err(err) = file.persist_noclobber(&path)
        && err.error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(err.error.into());
    }
    let meta = fs::symlink_metadata(&path)?;
    anyhow::ensure!(meta.is_file() && meta.len() <= 514, "invalid pairing file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        anyhow::ensure!(
            meta.permissions().mode() & 0o077 == 0,
            "pairing file must be private"
        );
    }
    let value = fs::read_to_string(&path)?.trim().to_owned();
    anyhow::ensure!(value.len() >= 32, "invalid pairing file");
    tracing::info!(path=%path.display(),"Local console service token file");
    Ok(value)
}
impl Access {
    pub(super) fn local_actor(&self) -> Option<Actor> {
        if self.shared {
            return None;
        }
        self.actors.first().map(|(_, actor)| actor.clone())
    }

    pub fn load(state: &AppState, root: &Path) -> anyhow::Result<Self> {
        let shared_file = std::env::var_os("PROXY_CONSOLE_PRINCIPALS");
        let shared = shared_file.is_some();
        let mut actors = Vec::new();
        let state_root = root.canonicalize()?;
        if let Some(path) = shared_file {
            let path = PathBuf::from(path).canonicalize()?;
            let entries: Vec<Principal> = serde_json::from_slice(&fs::read(&path)?)?;
            let mut homes = std::collections::HashSet::new();
            let mut spaces = Vec::<PathBuf>::new();
            for entry in entries {
                anyhow::ensure!(
                    !entry.id.is_empty()
                        && entry.id.len() <= 128
                        && matches!(entry.role.as_str(), "chat" | "admin"),
                    "invalid principal"
                );
                anyhow::ensure!(
                    entry.token_sha256.len() == 64
                        && entry.token_sha256.bytes().all(|x| x.is_ascii_hexdigit()),
                    "invalid token digest"
                );
                anyhow::ensure!(
                    entry.codex_home.is_absolute() && entry.workspace.is_absolute(),
                    "absolute paths required"
                );
                let home = entry.codex_home.canonicalize()?;
                let workspace = entry.workspace.canonicalize()?;
                anyhow::ensure!(
                    home.is_dir()
                        && workspace.is_dir()
                        && !workspace.starts_with(&home)
                        && !home.starts_with(&workspace),
                    "invalid approved directories"
                );
                anyhow::ensure!(
                    homes.insert(home.clone())
                        && !spaces
                            .iter()
                            .any(|w| w.starts_with(&workspace) || workspace.starts_with(w)),
                    "shared owners need disjoint private roots"
                );
                anyhow::ensure!(
                    !workspace.starts_with(&state_root)
                        && !state_root.starts_with(&workspace)
                        && !path.starts_with(&workspace),
                    "workspace contains service configuration or data"
                );
                anyhow::ensure!(
                    !homes
                        .iter()
                        .any(|h| h != &home && (h.starts_with(&home) || home.starts_with(h))),
                    "shared credential homes overlap"
                );
                spaces.push(workspace.clone());
                let auth = CredentialProvider::for_home(home)?;
                actors.push((
                    entry.token_sha256.to_ascii_lowercase(),
                    Actor {
                        id: entry.id,
                        admin: entry.role == "admin",
                        workspace,
                        credentials: auth,
                        provider: entry.open_code_provider,
                    },
                ));
            }
            // A workspace must never contain any principal's credential directory.
            anyhow::ensure!(
                spaces
                    .iter()
                    .all(|w| homes.iter().all(|h| !w.starts_with(h) && !h.starts_with(w))),
                "credential/workspace overlap"
            );
        } else {
            let token = service_token(root)?;
            let ws = std::env::var_os("PROXY_CONSOLE_WORKSPACE")
                .map(PathBuf::from)
                .unwrap_or(std::env::current_dir()?)
                .canonicalize()?;
            anyhow::ensure!(
                ws.is_dir() && !ws.starts_with(&state_root) && !state_root.starts_with(&ws),
                "choose PROXY_CONSOLE_WORKSPACE outside the console data directory"
            );
            let file = state.credentials.credential_path();
            let source = file.canonicalize().unwrap_or_else(|_| file.to_owned());
            anyhow::ensure!(
                !source.starts_with(&ws),
                "choose PROXY_CONSOLE_WORKSPACE outside the selected credential store"
            );
            if source.file_name().is_some_and(|name| name == "auth.json")
                && let Some(home) = source.parent()
            {
                anyhow::ensure!(
                    !ws.starts_with(home),
                    "choose PROXY_CONSOLE_WORKSPACE outside the credential home"
                );
            }
            let provider = std::env::var("PROXY_OPENCODE_PROVIDER")
                .ok()
                .map(|x| serde_json::from_str(&x))
                .transpose()?;
            actors.push((
                digest(token.as_bytes()),
                Actor {
                    id: "local".into(),
                    admin: true,
                    workspace: ws,
                    credentials: state.credentials.clone(),
                    provider,
                },
            ));
        }
        for (i, (hash, actor)) in actors.iter().enumerate() {
            anyhow::ensure!(
                !actors[..i]
                    .iter()
                    .any(|(h, a)| h == hash || a.id == actor.id),
                "duplicate principal"
            );
        }
        let origin = std::env::var("PROXY_CONSOLE_ORIGIN").ok();
        if let Some(s) = &origin {
            let url = reqwest::Url::parse(s)?;
            anyhow::ensure!(
                url.origin().ascii_serialization() == *s
                    && (url.scheme() == "https" || is_loopback(&url)),
                "invalid browser origin"
            );
        }
        let pairing = if shared {
            None
        } else {
            let mut p = Pairing {
                path: root.join("pairing-token"),
                hash: String::new(),
                expires: Instant::now(),
            };
            p.rotate()?;
            tracing::info!(path=%p.path.display(),"Read this file to pair the local console");
            Some(Mutex::new(p))
        };
        Ok(Self {
            shared,
            actors,
            origin,
            pairing,
            sessions: Mutex::new(HashMap::new()),
        })
    }
    fn by_token(&self, token: &str) -> Option<usize> {
        if !(32..=512).contains(&token.len()) {
            return None;
        }
        let hashed = digest(token.as_bytes());
        self.actors
            .iter()
            .position(|(key, _)| bool::from(key.as_bytes().ct_eq(hashed.as_bytes())))
    }
    pub fn check_origin(&self, h: &HeaderMap, required: bool) -> Result<()> {
        let Some(origin) = h.get("origin").and_then(|v| v.to_str().ok()) else {
            return if required {
                Err(Error::forbidden())
            } else {
                Ok(())
            };
        };
        if let Some(allowed) = &self.origin {
            return if allowed == origin {
                Ok(())
            } else {
                Err(Error::forbidden())
            };
        }
        let url = reqwest::Url::parse(origin).map_err(|_| Error::forbidden())?;
        let host = h.get("host").and_then(|v| v.to_str().ok()).unwrap_or("");
        let authority = origin.split_once("://").map(|(_, s)| s).unwrap_or("");
        if url.origin().ascii_serialization() == origin
            && is_loopback(&url)
            && authority.eq_ignore_ascii_case(host)
        {
            Ok(())
        } else {
            Err(Error::forbidden())
        }
    }
    pub fn login(&self, h: &HeaderMap, token: &str) -> Result<(Actor, String, String)> {
        self.check_origin(h, true)?;
        let i = if let Some(state) = &self.pairing {
            let mut p = state.lock().map_err(|_| Error::storage())?;
            let valid = p.expires > Instant::now()
                && bool::from(p.hash.as_bytes().ct_eq(digest(token.as_bytes()).as_bytes()));
            if p.expires <= Instant::now() || valid {
                p.rotate().map_err(|_| Error::storage())?;
            }
            if !valid {
                return Err(Error::unauthorized());
            }
            0
        } else {
            self.by_token(token).ok_or_else(Error::unauthorized)?
        };
        let cookie = secret();
        let csrf = secret();
        let mut sessions = self.sessions.lock().map_err(|_| Error::storage())?;
        sessions.retain(|_, s| s.expires > Instant::now());
        if sessions.len() >= 1024 {
            return Err(Error::busy());
        }
        sessions.insert(
            digest(cookie.as_bytes()),
            Session {
                actor: i,
                csrf: csrf.clone(),
                expires: Instant::now() + Duration::from_secs(8 * 3600),
            },
        );
        Ok((self.actors[i].1.clone(), cookie, csrf))
    }
    pub fn authenticate(&self, h: &HeaderMap, method: &Method) -> Result<(Actor, Option<String>)> {
        self.check_origin(h, false)?;
        if let Some(value) = h.get("authorization") {
            let idx = value
                .to_str()
                .ok()
                .and_then(|s| s.strip_prefix("Bearer "))
                .and_then(|s| self.by_token(s))
                .ok_or_else(Error::unauthorized)?;
            return Ok((self.actors[idx].1.clone(), None));
        }
        let key = cookie_key(h).ok_or_else(Error::unauthorized)?;
        let sessions = self.sessions.lock().map_err(|_| Error::storage())?;
        let s = sessions
            .get(&key)
            .filter(|x| x.expires > Instant::now())
            .ok_or_else(Error::unauthorized)?;
        if method != Method::GET && method != Method::HEAD {
            self.check_origin(h, true)?;
            let csrf = h
                .get("x-csrf-token")
                .and_then(|x| x.to_str().ok())
                .unwrap_or("");
            if !bool::from(csrf.as_bytes().ct_eq(s.csrf.as_bytes())) {
                return Err(Error::forbidden());
            }
        }
        Ok((self.actors[s.actor].1.clone(), Some(s.csrf.clone())))
    }
    pub fn refresh_pairing(&self) {
        if let Some(state) = &self.pairing
            && let Ok(mut p) = state.lock()
            && p.expires <= Instant::now()
        {
            let _ = p.rotate();
        }
    }
    pub fn logout(&self, h: &HeaderMap) -> Result<()> {
        self.authenticate(h, &Method::DELETE)?;
        if let Some(id) = cookie_key(h) {
            self.sessions
                .lock()
                .map_err(|_| Error::storage())?
                .remove(&id);
        }
        Ok(())
    }
    pub fn secure_cookie(&self) -> bool {
        self.origin
            .as_deref()
            .is_some_and(|s| s.starts_with("https://"))
    }
}
fn cookie_key(h: &HeaderMap) -> Option<String> {
    let cookies = h.get("cookie")?.to_str().ok()?;
    let v = cookies
        .split(';')
        .map(str::trim)
        .find_map(|s| s.strip_prefix("oproxy_session="))?;
    Some(digest(v.as_bytes()))
}
fn is_loopback(url: &reqwest::Url) -> bool {
    matches!(url.scheme(), "https" | "http")
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
}
