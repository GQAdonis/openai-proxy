mod editor;
mod target;

use anyhow::{Context, Result, anyhow, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use target::{describe, effective_revision, host_version};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Integration {
    OpencodePlugin,
    OpencodeProvider,
    OpencodeMcp,
    ClaudeMcp,
    CodexMcp,
    ZedMcp,
    VscodeMcp,
    ZedAcp,
    JetbrainsAcp,
    GenericAcp,
    GenericMcp,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Project,
    Global,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Stdio,
    Http,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetupRequest {
    pub integration: Integration,
    pub scope: Scope,
    #[serde(default)]
    pub transport: Transport,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Revision {
    hash: String,
    identity: String,
    exists: bool,
}
#[derive(Clone)]
struct Preview {
    actor: String,
    request: SetupRequest,
    expires: u64,
    path: PathBuf,
    base: Revision,
    layers: String,
    after: String,
    version: String,
}
// An inverse splice contains only characters displaced at the insertion site. The rest of a user's
// settings, including existing credentials, never enter preview output or the persisted receipt.
#[derive(Serialize, Deserialize)]
struct Receipt {
    actor: String,
    request: SetupRequest,
    path: PathBuf,
    base: Revision,
    after: String,
    offset: usize,
    inserted: usize,
    restore: String,
    undone: bool,
}

pub struct Provisioner {
    project: PathBuf,
    home: PathBuf,
    proxy_port: u16,
    mcp_port: u16,
    base_url: Option<String>,
    pending: Mutex<HashMap<String, Preview>>,
}
impl Provisioner {
    pub fn new(project: PathBuf, home: PathBuf, proxy_port: u16, mcp_port: u16) -> Self {
        Self {
            project: project.canonicalize().unwrap_or(project),
            home: home.canonicalize().unwrap_or(home),
            proxy_port,
            mcp_port,
            base_url: None,
            pending: Mutex::new(HashMap::new()),
        }
    }
    pub fn with_base_url(mut self, url: Option<&str>) -> Result<Self> {
        if let Some(url) = url {
            let parsed = reqwest::Url::parse(url)?;
            ensure!(
                parsed.username().is_empty()
                    && parsed.password().is_none()
                    && parsed.query().is_none()
                    && parsed.fragment().is_none()
                    && (parsed.scheme() == "https"
                        || (parsed.scheme() == "http"
                            && matches!(
                                parsed.host_str(),
                                Some("localhost" | "127.0.0.1" | "[::1]")
                            ))),
                "base URL must be a trusted HTTPS or loopback HTTP endpoint"
            );
            self.base_url = Some(url.to_owned());
        }
        Ok(self)
    }

    fn approved_root(&self, scope: Scope, file: &Path) -> Result<PathBuf> {
        if scope == Scope::Project {
            return Ok(self.project.clone());
        }
        if file.starts_with(&self.home) {
            return Ok(self.home.clone());
        }
        // These secondary scopes are established by the service environment, never by a request.
        for key in ["XDG_CONFIG_HOME", "CODEX_HOME"] {
            if let Some(root) = std::env::var_os(key).map(PathBuf::from)
                && root.is_absolute()
                && root.is_dir()
                && file.starts_with(&root)
                && root.canonicalize()? == root
            {
                return Ok(root);
            }
        }
        bail!("configuration is outside the approved service-host directories")
    }
    pub fn export(&self, request: SetupRequest) -> Result<Value> {
        let target = describe(self, &request)?;
        Ok(
            json!({"status":"export","integration":request.integration,"scope":request.scope,
            "target":"service_host","export":target.export(),"changes":[]}),
        )
    }
    pub fn portable_export(request: SetupRequest) -> Result<Value> {
        use Integration::*;
        let http = request.transport == Transport::Http;
        let acp = matches!(request.integration, ZedAcp | JetbrainsAcp | GenericAcp);
        ensure!(!(http && acp), "ACP requires stdio");
        let url = "http://127.0.0.1:8081/mcp";
        let command = "/absolute/path/to/openai-proxy";
        let args = ["serve", if acp { "--acp-stdio" } else { "--mcp-stdio" }];
        let native = json!({"command":command,"args":args});
        let mcp = if http {
            json!({"url":url})
        } else {
            native.clone()
        };
        let example = match request.integration {
            OpencodePlugin => json!({"plugins":["/absolute/path/to/plugin"]}),
            OpencodeProvider => json!({"providers":{"openai-proxy":{"name":"OpenAI Proxy",
                "package":"@opencode/ai/providers/openai-compatible","settings":{"baseURL":"http://127.0.0.1:8181/api/v1/drivers/codex/v1","apiKey":"{env:PROXY_CONSOLE_TOKEN}"},
                "models":{"gpt-6.1-sol":{"capabilities":{"tools":false,"input":["text"],"output":["text"]}}}}}}),
            OpencodeMcp => {
                json!({"mcp":{"servers":{"openai-proxy": if http { json!({"type":"remote","url":url}) }
                else { json!({"type":"local","command":[command,"serve","--mcp-stdio"]}) }}}})
            }
            ClaudeMcp | VscodeMcp => {
                let key = if request.integration == ClaudeMcp {
                    "mcpServers"
                } else {
                    "servers"
                };
                json!({(key):{"openai-proxy": if http {json!({"type":"http","url":url})}
                    else {json!({"type":"stdio","command":command,"args":args})}}})
            }
            CodexMcp => json!({"mcp_servers":{"openai-proxy":mcp}}),
            ZedMcp => json!({"context_servers":{"openai-proxy":mcp}}),
            ZedAcp => {
                json!({"agent_servers":{"openai-proxy":{"type":"custom","command":command,"args":args}}})
            }
            JetbrainsAcp => json!({"agent_servers":{"openai-proxy":native}}),
            GenericMcp | GenericAcp => mcp,
        };
        Ok(
            json!({"status":"export","target":"visitor","integration":request.integration,"scope":request.scope,"export":example,"changes":[]}),
        )
    }

    pub fn preview(&self, actor: &str, request: SetupRequest) -> Result<Value> {
        let t = describe(self, &request)?;
        let export = t.export();
        let mut response = json!({"status":"export","integration":request.integration,"scope":request.scope,
            "target":"service_host","export":export,"changes":[]});
        if t.export_only {
            response["reason"] = json!(
                if t.client == "zed" && request.transport == Transport::Http {
                    "Configure authentication in Zed or use stdio. The writer cannot store a bearer secret."
                } else {
                    "Use the exported descriptor on a supported target."
                }
            );
            return Ok(response);
        }
        let id = Uuid::new_v4().to_string();
        let prepare = (|| -> Result<_> {
            safe_path(&t.root, &t.file)?;
            let version = host_version(t.client)?;
            let layers = effective_revision(self, &t)?;
            let raw = source(&t.file)?;
            let before = revision(&t.file, raw.as_deref())?;
            let after = t.edit(raw.as_deref())?;
            let preview = Preview {
                actor: digest(actor.as_bytes()),
                request: request.clone(),
                expires: now() + 300,
                path: t.file.clone(),
                base: before,
                layers,
                after: digest(after.as_bytes()),
                version: version.clone(),
            };
            Ok((preview, version))
        })();
        match prepare {
            Ok((preview, version)) => {
                let mut pending = self
                    .pending
                    .lock()
                    .map_err(|_| anyhow!("provisioning state unavailable"))?;
                pending.retain(|_, v| v.expires >= now());
                ensure!(pending.len() < 256, "too many active provisioning previews");
                response["status"] = json!("ready");
                response["id"] = json!(id);
                response["path"] = json!(t.file);
                response["version"] = json!(version);
                response["hostEvidence"] = json!(if t.opencode && version != "2.3.0" {
                    "in-range, unverified host"
                } else {
                    "version detected"
                });
                response["changes"] = if preview.base.hash != preview.after {
                    json!([{"operation":if t.append{"append"}else{"add"},"keys":t.keys,"value":t.entry}])
                } else {
                    json!([])
                };
                pending.insert(id, preview);
            }
            Err(reason) => {
                response["status"] = json!("blocked");
                response["reason"] = json!(reason.to_string());
            }
        }
        Ok(response)
    }
    fn receipt_path(&self, id: &str) -> Result<PathBuf> {
        let id = Uuid::parse_str(id).context("invalid provisioning identifier")?;
        Ok(self
            .home
            .join(".local/state/openai-proxy/provision")
            .join(format!("{id}.json")))
    }
    fn load(&self, id: &str, actor: &str) -> Result<Option<Receipt>> {
        let path = self.receipt_path(id)?;
        safe_path(&self.home, &path)?;
        let Some(raw) = source(&path)? else {
            return Ok(None);
        };
        let receipt: Receipt = serde_json::from_str(&raw)?;
        ensure!(
            receipt.actor == digest(actor.as_bytes()),
            "unknown provisioning identifier"
        );
        Ok(Some(receipt))
    }
    fn save(&self, id: &str, receipt: &Receipt) -> Result<()> {
        let path = self.receipt_path(id)?;
        safe_path(&self.home, &path)?;
        create_parents(&self.home, path.parent().unwrap())?;
        replace(&path, &serde_json::to_vec(receipt)?)
    }
    pub fn apply(&self, actor: &str, id: &str) -> Result<Value> {
        let lock = File::open(&self.home)?;
        lock.lock_exclusive()?;
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow!("provisioning state unavailable"))?;
        if let Some(receipt) = self.load(id, actor)? {
            let t = describe(self, &receipt.request)?;
            ensure!(
                t.file == receipt.path && !receipt.undone,
                "preview is no longer active"
            );
            safe_path(&t.root, &t.file)?;
            let current = source(&t.file)?;
            if current
                .as_ref()
                .is_some_and(|s| digest(s.as_bytes()) == receipt.after)
            {
                return Ok(json!({"status":"applied","id":id}));
            }
            // An intent was stored before rename. Only the original live preview may finish it.
        }
        let preview = pending
            .get(id)
            .ok_or_else(|| anyhow!("unknown or expired preview"))?;
        ensure!(
            preview.expires >= now() && preview.actor == digest(actor.as_bytes()),
            "unknown or expired preview"
        );
        let t = describe(self, &preview.request)?;
        ensure!(
            t.file == preview.path && !t.export_only,
            "target changed since preview"
        );
        safe_path(&t.root, &t.file)?;
        ensure!(
            host_version(t.client)? == preview.version
                && effective_revision(self, &t)? == preview.layers,
            "client or effective configuration changed"
        );
        let before = source(&t.file)?;
        ensure!(
            revision(&t.file, before.as_deref())? == preview.base,
            "target changed since preview"
        );
        let after = t.edit(before.as_deref())?;
        ensure!(
            digest(after.as_bytes()) == preview.after,
            "proposed settings changed"
        );
        let (offset, inserted, restore) = inverse(before.as_deref().unwrap_or(""), &after)?;
        let receipt = Receipt {
            actor: preview.actor.clone(),
            request: preview.request.clone(),
            path: t.file.clone(),
            base: preview.base.clone(),
            after: preview.after.clone(),
            offset,
            inserted,
            restore,
            undone: false,
        };
        self.save(id, &receipt)?;
        if before.as_deref() != Some(&after) {
            create_parents(&t.root, t.file.parent().unwrap())?;
            safe_path(&t.root, &t.file)?;
            ensure!(
                revision(&t.file, source(&t.file)?.as_deref())? == preview.base,
                "target changed before write"
            );
            replace(&t.file, after.as_bytes())?;
        }
        Ok(json!({"status":"applied","id":id}))
    }
    pub fn undo(&self, actor: &str, id: &str) -> Result<Value> {
        let lock = File::open(&self.home)?;
        lock.lock_exclusive()?;
        let mut receipt = self
            .load(id, actor)?
            .ok_or_else(|| anyhow!("unknown applied change"))?;
        let root = self.approved_root(receipt.request.scope, &receipt.path)?;
        safe_path(&root, &receipt.path)?;
        if receipt.undone {
            return Ok(json!({"status":"undone","id":id}));
        }
        let current = source(&receipt.path)?;
        let hash = digest(current.as_deref().unwrap_or("").as_bytes());
        if hash != receipt.base.hash || current.is_some() != receipt.base.exists {
            ensure!(hash == receipt.after, "a later edit conflicts with undo");
            let mut restored = current.ok_or_else(|| anyhow!("target disappeared"))?;
            let end = receipt
                .offset
                .checked_add(receipt.inserted)
                .ok_or_else(|| anyhow!("invalid recovery record"))?;
            ensure!(
                end <= restored.len()
                    && restored.is_char_boundary(receipt.offset)
                    && restored.is_char_boundary(end),
                "invalid recovery record"
            );
            restored.replace_range(receipt.offset..end, &receipt.restore);
            ensure!(
                digest(restored.as_bytes()) == receipt.base.hash,
                "recovery content mismatch"
            );
            if receipt.base.exists {
                replace(&receipt.path, restored.as_bytes())?;
            } else {
                fs::remove_file(&receipt.path)?;
                File::open(receipt.path.parent().unwrap())?.sync_all()?;
            }
        }
        receipt.undone = true;
        self.save(id, &receipt)?;
        Ok(json!({"status":"undone","id":id}))
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn safe_path(root: &Path, path: &Path) -> Result<()> {
    ensure!(
        root.is_absolute() && root.is_dir(),
        "approved scope is unavailable"
    );
    let suffix = path
        .strip_prefix(root)
        .context("target is outside the approved scope")?;
    let mut candidate = root.to_owned();
    for component in suffix.components() {
        let Component::Normal(name) = component else {
            bail!("invalid target component")
        };
        candidate.push(name);
        match fs::symlink_metadata(&candidate) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "a symbolic link is outside the supported write contract"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn create_parents(root: &Path, dir: &Path) -> Result<()> {
    let mut path = root.to_owned();
    for part in dir.strip_prefix(root)?.components() {
        path.push(part);
        if !path.exists() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            if let Err(error) = builder.create(&path) {
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(error.into());
                }
            }
        }
        safe_path(root, &path)?;
    }
    Ok(())
}
fn source(path: &Path) -> Result<Option<String>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    ensure!(
        meta.is_file() && meta.len() <= 1_048_576,
        "configuration must be a bounded regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(meta.nlink() == 1, "hard-linked files cannot be modified");
    }
    let mut data = String::new();
    File::open(path)?
        .take(1_048_577)
        .read_to_string(&mut data)?;
    ensure!(
        data.len() <= 1_048_576,
        "configuration exceeded its size limit"
    );
    Ok(Some(data))
}
fn revision(path: &Path, raw: Option<&str>) -> Result<Revision> {
    let identity = if raw.is_some() {
        let meta = fs::symlink_metadata(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            format!("{}:{}:{}", meta.dev(), meta.ino(), meta.mode())
        }
        #[cfg(not(unix))]
        {
            format!("{:?}", meta.modified()?)
        }
    } else {
        String::new()
    };
    Ok(Revision {
        hash: digest(raw.unwrap_or("").as_bytes()),
        exists: raw.is_some(),
        identity,
    })
}
fn inverse(before: &str, after: &str) -> Result<(usize, usize, String)> {
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(x, y)| x == y)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let restore = &before[prefix..before.len() - suffix];
    ensure!(
        restore.len() <= 4096
            && restore
                .chars()
                .all(|c| c.is_whitespace() || ",]}".contains(c)),
        "editor would replace existing user content"
    );
    Ok((prefix, after.len() - prefix - suffix, restore.to_owned()))
}
fn replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("missing target parent")?;
    let mut temporary = NamedTempFile::new_in(dir)?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(meta.is_file(), "unexpected target type");
        temporary.as_file().set_permissions(meta.permissions())?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    File::open(dir)?.sync_all()?;
    Ok(())
}
