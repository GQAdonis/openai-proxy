use super::*;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use toml_edit::{DocumentMut, Item, Table};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Preview {
    actor: String,
    field: String,
    path: PathBuf,
    before: String,
    after: String,
    existed: bool,
    revision: String,
    applied_revision: Option<String>,
    undone: bool,
    expires: Instant,
}

fn path(s: &Server) -> PathBuf {
    s.app
        .runtime
        .config_path
        .clone()
        .unwrap_or_else(|| crate::config::config_dir().join("config.toml"))
}
fn contents(path: &Path) -> Result<(String, bool, String)> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((String::new(), false, String::new()));
        }
        Err(_) => return Err(Error::storage()),
    };
    if !meta.is_file() || meta.len() > 256 * 1024 {
        return Err(Error::conflict());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(Error::conflict());
        }
    }
    let text = fs::read_to_string(path).map_err(|_| Error::storage())?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}:{}", meta.dev(), meta.ino(), meta.mode())
    };
    #[cfg(not(unix))]
    let identity = format!("{:?}", meta.modified().map_err(|_| Error::storage())?);
    let revision = format!("{}:{}", identity, access::digest(text.as_bytes()));
    Ok((text, true, revision))
}

pub(super) fn file_config(s: &Server) -> Option<toml::Table> {
    let (raw, _, _) = contents(&path(s)).ok()?;
    toml::from_str(&raw).ok()
}

fn approved(field: &str, value: &Value) -> bool {
    match field {
        "server.port" => value.as_u64().is_some_and(|port| port > 0 && port <= 65535),
        "backend.wire_api" => matches!(value.as_str(), Some("responses" | "chat")),
        "skills.max_injected" => value.as_u64().is_some_and(|n| n <= 32),
        "modes.a2a" => value.is_boolean(),
        "memory.enabled" => cfg!(feature = "memory") && value.is_boolean(),
        "hooks.destination" => value
            .as_str()
            .is_some_and(|id| crate::hooks::WebhookHooks::trusted_destinations().contains_key(id)),
        _ => false,
    }
}
pub(super) fn mutable(s: &Server, field: &str) -> bool {
    let source = s
        .app
        .runtime
        .sources
        .get(field)
        .map(String::as_str)
        .unwrap_or("default");
    !matches!(source, "cli" | "environment")
        && !(field == "memory.enabled" && !cfg!(feature = "memory"))
        && !(field == "hooks.destination"
            && (s.app.runtime.config.hooks.config_path.is_some()
                || crate::hooks::WebhookHooks::trusted_destinations().is_empty()))
}

fn change(before: &str, field: &str, value: &Value) -> Result<String> {
    let (section, key) = field.split_once('.').ok_or_else(Error::bad)?;
    let mut doc = before
        .parse::<DocumentMut>()
        .map_err(|_| Error::conflict())?;
    if !doc.contains_key(section) {
        doc[section] = Item::Table(Table::new())
    }
    let table = doc[section]
        .as_table_like_mut()
        .ok_or_else(Error::conflict)?;
    let mut replacement = if let Some(i) = value.as_i64() {
        toml_edit::Value::from(i)
    } else if let Some(v) = value.as_bool() {
        toml_edit::Value::from(v)
    } else {
        toml_edit::Value::from(value.as_str().ok_or_else(Error::bad)?)
    };
    if let Some(old) = table.get(key) {
        *replacement.decor_mut() = old.as_value().ok_or_else(Error::conflict)?.decor().clone();
    }
    table.insert(key, Item::Value(replacement));
    let after = doc.to_string();
    toml::from_str::<crate::config::ProxyConfig>(&after).map_err(|_| Error::bad())?;
    Ok(after)
}
fn replace(path: &Path, expected: &str, contents_new: Option<&str>) -> Result<String> {
    valid_path(path)?;
    let parent = path.parent().ok_or_else(Error::storage)?;
    if !parent.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(parent).map_err(|_| Error::storage())?;
    }
    let target = fs::canonicalize(parent)
        .map_err(|_| Error::storage())?
        .join(path.file_name().ok_or_else(Error::storage)?);
    if contents(&target)?.2 != expected {
        return Err(Error::conflict());
    }
    if let Some(raw) = contents_new {
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::storage())?;
        if let Ok(meta) = fs::symlink_metadata(&target) {
            temp.as_file()
                .set_permissions(meta.permissions())
                .map_err(|_| Error::storage())?;
        }
        temp.write_all(raw.as_bytes())
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|_| Error::storage())?;
        if contents(&target)?.2 != expected {
            return Err(Error::conflict());
        }
        if expected.is_empty() {
            temp.persist_noclobber(&target)
                .map_err(|_| Error::conflict())?;
        } else {
            temp.persist(&target).map_err(|_| Error::storage())?;
        }
    } else {
        fs::remove_file(&target).map_err(|_| Error::storage())?;
    }
    #[cfg(unix)]
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::storage())?;
    Ok(contents(&target)?.2)
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    actor: String,
    target: String,
    base_exists: bool,
    before: String,
    after: String,
    offset: usize,
    inserted: usize,
    displaced: String,
    completed_revision: Option<String>,
    undone: bool,
}

fn receipt_path(server: &Server, id: &str) -> Result<PathBuf> {
    let id = Uuid::parse_str(id).map_err(|_| Error::bad())?;
    let root = server.root.join("config-edits");
    access::private_dir(&root).map_err(|_| Error::storage())?;
    Ok(root.join(format!("{id}.json")))
}

fn valid_path(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(parent)
            && meta.is_symlink()
        {
            return Err(Error::conflict());
        }
    }
    Ok(())
}

impl Receipt {
    fn new(p: &Preview) -> Result<Self> {
        let prefix = p
            .before
            .chars()
            .zip(p.after.chars())
            .take_while(|(a, b)| a == b)
            .map(|(ch, _)| ch.len_utf8())
            .sum::<usize>();
        let suffix = p.before[prefix..]
            .chars()
            .rev()
            .zip(p.after[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(ch, _)| ch.len_utf8())
            .sum::<usize>();
        let displaced = &p.before[prefix..p.before.len() - suffix];
        if displaced.len() > 2048 {
            return Err(Error::conflict());
        }
        Ok(Self {
            actor: access::digest(p.actor.as_bytes()),
            target: access::digest(p.path.as_os_str().as_encoded_bytes()),
            base_exists: p.existed,
            before: access::digest(p.before.as_bytes()),
            after: access::digest(p.after.as_bytes()),
            offset: prefix,
            inserted: p.after.len() - prefix - suffix,
            displaced: displaced.into(),
            completed_revision: p.applied_revision.clone(),
            undone: false,
        })
    }

    fn recover(server: &Server, actor: &Actor, id: &str, op: &str) -> Result<Value> {
        let path = receipt_path(server, id)?;
        let mut receipt: Self = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or_else(Error::conflict)?;
        let target = super::configure::path(server);
        valid_path(&target)?;
        if receipt.actor != access::digest(actor.id.as_bytes())
            || receipt.target != access::digest(target.as_os_str().as_encoded_bytes())
        {
            return Err(Error::conflict());
        }
        let (current, exists, revision) = contents(&target)?;
        let digest = access::digest(current.as_bytes());
        match op {
            "apply"
                if !receipt.undone
                    && exists
                    && digest == receipt.after
                    && receipt
                        .completed_revision
                        .as_ref()
                        .is_none_or(|rev| *rev == revision) =>
            {
                return Ok(json!({"status":"applied","id":id,"activation":"service restart"}));
            }
            "undo" if !receipt.undone => {
                if digest != receipt.before || exists != receipt.base_exists {
                    if digest != receipt.after
                        || receipt
                            .completed_revision
                            .as_ref()
                            .is_some_and(|rev| *rev != revision)
                    {
                        return Err(Error::conflict());
                    }
                    let end = receipt
                        .offset
                        .checked_add(receipt.inserted)
                        .ok_or_else(Error::conflict)?;
                    if !current.is_char_boundary(receipt.offset) || !current.is_char_boundary(end) {
                        return Err(Error::conflict());
                    }
                    let mut old = current;
                    old.replace_range(receipt.offset..end, &receipt.displaced);
                    if access::digest(old.as_bytes()) != receipt.before {
                        return Err(Error::conflict());
                    }
                    replace(
                        &target,
                        &revision,
                        receipt.base_exists.then_some(old.as_str()),
                    )?;
                }
                receipt.undone = true;
                access::atomic_json(&path, &receipt)?;
            }
            "undo" => {}
            _ => return Err(Error::conflict()),
        }
        Ok(json!({"status":"undone","id":id,"activation":"service restart"}))
    }
}

pub(super) struct Changes(Mutex<HashMap<String, Preview>>);
impl Default for Changes {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}
impl Changes {
    pub(super) fn dispatch(
        &self,
        s: &Server,
        actor: &Actor,
        op: &str,
        body: Value,
    ) -> Result<Value> {
        let mut list = self.0.lock().map_err(|_| Error::storage())?;
        list.retain(|_, p| p.expires > Instant::now());
        if op == "preview" {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Edit {
                field: String,
                value: Value,
            }
            let e: Edit = serde_json::from_value(body).map_err(|_| Error::bad())?;
            if !approved(&e.field, &e.value) {
                return Err(Error::bad());
            }
            if !mutable(s, &e.field) {
                return Err(Error::conflict());
            }
            if list.len() >= 32 {
                return Err(Error::busy());
            }
            let path = path(s);
            valid_path(&path)?;
            let (before, existed, revision) = contents(&path)?;
            let after = change(&before, &e.field, &e.value)?;
            let id = Uuid::new_v4().to_string();
            let response = json!({"status":"ready","id":id,"changes":[{"field":e.field,"value":e.value}],"activation":"service restart"});
            list.insert(
                id,
                Preview {
                    actor: actor.id.clone(),
                    field: e.field,
                    path,
                    before,
                    after,
                    existed,
                    revision,
                    applied_revision: None,
                    undone: false,
                    expires: Instant::now() + Duration::from_secs(300),
                },
            );
            return Ok(response);
        }
        let Id { id } = serde_json::from_value(body).map_err(|_| Error::bad())?;
        if !list.contains_key(&id) {
            return Receipt::recover(s, actor, &id, op);
        }
        let p = list
            .get_mut(&id)
            .filter(|p| p.actor == actor.id)
            .ok_or_else(Error::conflict)?;
        let (_, _, revision) = contents(&p.path)?;
        let status = match op {
            "apply" => {
                if p.undone || !mutable(s, &p.field) {
                    return Err(Error::conflict());
                }
                if let Some(applied) = &p.applied_revision {
                    if revision != *applied {
                        return Err(Error::conflict());
                    }
                } else {
                    if revision != p.revision {
                        return Err(Error::conflict());
                    }
                    let receipt_path = receipt_path(s, &id)?;
                    let mut receipt = Receipt::new(p)?;
                    access::atomic_json(&receipt_path, &receipt)?;
                    p.applied_revision = Some(if p.before == p.after {
                        if revision != p.revision {
                            return Err(Error::conflict());
                        }
                        revision
                    } else {
                        replace(&p.path, &p.revision, Some(&p.after))?
                    });
                    receipt.completed_revision = p.applied_revision.clone();
                    access::atomic_json(&receipt_path, &receipt)?;
                }
                p.expires = Instant::now() + Duration::from_secs(3600);
                "applied"
            }
            "undo" => {
                Receipt::recover(s, actor, &id, op)?;
                p.undone = true;
                "undone"
            }
            _ => return Err(Error::bad()),
        };
        Ok(json!({"status":status,"id":id,"activation":"service restart"}))
    }
}
