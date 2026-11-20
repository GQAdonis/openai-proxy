use super::{Error, Result, access::digest};
use jsonschema::{Registry, Validator};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

const CATALOG: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";
const AGENT: &str = include_str!("../../frontend/src/schemas/agent_to_renderer.json");
const USER: &str = include_str!("../../frontend/src/schemas/renderer_to_agent.json");
const COMMON: &str = include_str!("../../frontend/src/schemas/common_types.json");
const BASIC: &str = include_str!("../../frontend/src/schemas/catalog.json");

fn validators() -> Option<&'static (Validator, Validator)> {
    static VALIDATORS: OnceLock<Option<(Validator, Validator)>> = OnceLock::new();
    VALIDATORS
        .get_or_init(|| {
            let mut registry = Registry::new();
            for raw in [COMMON, BASIC, AGENT, USER] {
                let schema: Value = serde_json::from_str(raw).ok()?;
                let id = schema["$id"].as_str()?.to_owned();
                registry = registry.add(id, schema).ok()?;
            }
            let ready = registry.prepare().ok()?;
            let build = |schema| {
                jsonschema::options()
                    .with_registry(&ready)
                    .should_validate_formats(true)
                    .build(&serde_json::from_str::<Value>(schema).ok()?)
                    .ok()
            };
            Some((build(AGENT)?, build(USER)?))
        })
        .as_ref()
}
fn id(v: &Value) -> Result<&str> {
    v.as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 100
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                && !matches!(*s, "__proto__" | "constructor" | "prototype")
        })
        .ok_or_else(Error::bad)
}
fn pointer(path: &str) -> Result<Vec<String>> {
    if path == "/" || path.is_empty() {
        return Ok(Vec::new());
    }
    if !path.starts_with('/') || path.len() > 512 {
        return Err(Error::bad());
    }
    path[1..]
        .split('/')
        .map(|p| {
            let p = p.replace("~1", "/").replace("~0", "~");
            if matches!(p.as_str(), "__proto__" | "constructor" | "prototype") || p.is_empty() {
                Err(Error::bad())
            } else {
                Ok(p)
            }
        })
        .collect()
}
fn patch_data(data: &mut Value, path: &str, value: Value) -> Result<()> {
    let keys = pointer(path)?;
    if keys.is_empty() {
        if !value.is_object() {
            return Err(Error::bad());
        }
        *data = value;
        return Ok(());
    }
    let mut parent = data;
    for key in &keys[..keys.len() - 1] {
        parent = parent
            .as_object_mut()
            .ok_or_else(Error::bad)?
            .entry(key.clone())
            .or_insert_with(|| json!({}));
    }
    let obj = parent.as_object_mut().ok_or_else(Error::bad)?;
    let key = &keys[keys.len() - 1];
    if value.is_null() {
        obj.remove(key);
    } else {
        obj.insert(key.clone(), value);
    }
    Ok(())
}
fn bounded(v: &Value, depth: usize) -> bool {
    if depth > 20 {
        return false;
    }
    match v {
        Value::Array(a) => a.len() <= 256 && a.iter().all(|v| bounded(v, depth + 1)),
        Value::Object(o) => {
            o.len() <= 128
                && o.iter().all(|(k, v)| {
                    !matches!(k.as_str(), "__proto__" | "prototype" | "constructor")
                        && bounded(v, depth + 1)
                })
        }
        _ => true,
    }
}
fn dynamic(v: &Value, data: &Value) -> Result<Value> {
    if let Some(s) = v.as_str() {
        if s.len() > 8192 {
            return Err(Error::bad());
        }
        return Ok(v.clone());
    }
    if let Some(p) = v.get("@path").and_then(Value::as_str) {
        if v.as_object().map(|o| o.len()) != Some(1) {
            return Err(Error::bad());
        }
        pointer(p)?;
        return Ok(data.pointer(p).cloned().unwrap_or(Value::Null));
    }
    Err(Error::bad())
}
fn component(c: &Value, data: &Value) -> Result<Vec<String>> {
    let kind = c["component"].as_str().ok_or_else(Error::bad)?;
    let fields: &[&str] = match kind {
        "Button" => &["child", "action"],
        "Column" | "Row" => &["children"],
        "Card" => &["child"],
        "Text" => &["text"],
        _ => return Err(Error::bad()),
    };
    let map = c.as_object().ok_or_else(Error::bad)?;
    if fields.iter().any(|k| !map.contains_key(*k)) {
        return Err(Error::bad());
    }
    if map.keys().any(|k| {
        !matches!(
            k.as_str(),
            "id" | "component" | "@accessibility" | "@catalogId"
        ) && !fields.contains(&k.as_str())
    }) {
        return Err(Error::bad());
    }
    if c.get("@catalogId")
        .is_some_and(|v| v.as_str() != Some(CATALOG))
    {
        return Err(Error::bad());
    }
    if let Some(a) = c.get("@accessibility") {
        for (k, v) in a.as_object().ok_or_else(Error::bad)? {
            if !matches!(k.as_str(), "label" | "description") {
                return Err(Error::bad());
            }
            dynamic(v, data)?;
        }
    }
    if kind == "Text" {
        dynamic(&c["text"], data)?;
    }
    if kind == "Button" {
        let action = c["action"]
            .as_object()
            .filter(|m| m.len() == 1)
            .ok_or_else(Error::bad)?;
        let event = action
            .get("event")
            .and_then(Value::as_object)
            .ok_or_else(Error::bad)?;
        if !matches!(
            event.get("name").and_then(Value::as_str),
            Some("choose" | "submit")
        ) || event
            .keys()
            .any(|k| !matches!(k.as_str(), "name" | "context"))
        {
            return Err(Error::bad());
        }
        if let Some(ctx) = event.get("context") {
            action_context(ctx, data)?;
        }
    }
    let mut children = Vec::new();
    if let Some(v) = c.get("child") {
        children.push(id(v)?.to_owned());
    }
    if let Some(list) = c.get("children") {
        for v in list.as_array().ok_or_else(Error::bad)? {
            children.push(id(v)?.to_owned());
        }
    }
    Ok(children)
}
fn action_context(ctx: &Value, data: &Value) -> Result<Value> {
    let mut out = serde_json::Map::new();
    for (key, v) in ctx.as_object().ok_or_else(Error::bad)? {
        let resolved = if v.get("@path").is_some() {
            dynamic(v, data)?
        } else if v.is_object() || v.is_array() {
            return Err(Error::bad());
        } else {
            v.clone()
        };
        out.insert(key.clone(), resolved);
    }
    Ok(Value::Object(out))
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct Surface {
    components: BTreeMap<String, Value>,
    data: Value,
}
impl Surface {
    fn validate(&self) -> Result<()> {
        if self.components.len() > 128 {
            return Err(Error::bad());
        }
        let edges = self
            .components
            .iter()
            .map(|(k, v)| Ok((k.clone(), component(v, &self.data)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        fn visit(
            k: &str,
            edges: &BTreeMap<String, Vec<String>>,
            seen: &mut BTreeSet<String>,
            depth: usize,
        ) -> Result<()> {
            if depth > 20 || !seen.insert(k.to_owned()) {
                return Err(Error::bad());
            }
            for child in edges.get(k).ok_or_else(Error::bad)? {
                visit(child, edges, seen, depth + 1)?;
            }
            Ok(())
        }
        {
            let mut seen = BTreeSet::new();
            visit("root", &edges, &mut seen, 0)?;
            if seen.len() != edges.len() {
                return Err(Error::bad());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct Snapshot {
    #[serde(default)]
    turn: String,
    content: Value,
    live: BTreeMap<String, Surface>,
    used: BTreeSet<String>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub(super) struct State {
    messages: BTreeMap<String, Snapshot>,
}
impl State {
    pub fn snapshots(&self, turn: &str) -> Vec<Value> {
        self.messages
            .iter()
            .filter(|(_, s)| s.turn == turn)
            .map(|(key, s)| json!({"type":"surface","messageId":key,"content":s.content}))
            .collect()
    }
    /// A snapshot is a complete ordered replacement. Build it off to the side before publication.
    pub fn accept(&mut self, v: &Value, turn: &str) -> Result<Value> {
        let mid = id(&v["messageId"])?;
        if self.messages.len() >= 16 && !self.messages.contains_key(mid) {
            return Err(Error::busy());
        }
        let content = &v["content"];
        let ops = content["a2ui_operations"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 64)
            .ok_or_else(Error::bad)?;
        if content.as_object().is_none_or(|m| m.len() != 1)
            || content.to_string().len() > 65536
            || !bounded(content, 0)
        {
            return Err(Error::bad());
        }
        let (schema, _) = validators().ok_or_else(Error::unavailable)?;
        let mut snap = Snapshot {
            turn: turn.to_owned(),
            content: content.clone(),
            ..Default::default()
        };
        let mut created = BTreeSet::new();
        for op in ops {
            if op["version"] != "1.0" || !schema.is_valid(op) {
                return Err(Error::bad());
            }
            let (kind, body) = op
                .as_object()
                .and_then(|o| o.iter().find(|(k, _)| k.as_str() != "version"))
                .ok_or_else(Error::bad)?;
            let sid = id(&body["surfaceId"])?;
            if self
                .messages
                .iter()
                .any(|(key, s)| key != mid && s.live.contains_key(sid))
            {
                return Err(Error::conflict());
            }
            match kind.as_str() {
                "createSurface" => {
                    if body["catalogId"] != CATALOG
                        || body["sendDataModel"] == true
                        || !created.insert(sid)
                        || created.len() > 8
                    {
                        return Err(Error::bad());
                    }
                    snap.live.insert(
                        sid.into(),
                        Surface {
                            data: body.get("dataModel").cloned().unwrap_or_else(|| json!({})),
                            ..Default::default()
                        },
                    );
                }
                "updateComponents" => {
                    if !snap.live.contains_key(sid) {
                        return Err(Error::bad());
                    }
                }
                "updateDataModel" => {
                    let surface = snap.live.get_mut(sid).ok_or_else(Error::bad)?;
                    patch_data(
                        &mut surface.data,
                        body["path"].as_str().unwrap_or("/"),
                        body["value"].clone(),
                    )?;
                }
                "deleteSurface" => {
                    if snap.live.remove(sid).is_none() {
                        return Err(Error::bad());
                    }
                }
                _ => return Err(Error::bad()),
            }
            if let Some(items) = body.get("components") {
                let surface = snap.live.get_mut(sid).ok_or_else(Error::bad)?;
                let mut seen = BTreeSet::new();
                for c in items.as_array().ok_or_else(Error::bad)? {
                    let key = id(&c["id"])?;
                    if !seen.insert(key) {
                        return Err(Error::bad());
                    }
                    surface.components.insert(key.into(), c.clone());
                }
            }
        }
        for surface in snap.live.values() {
            surface.validate()?;
        }
        if let Some(previous) = self.messages.get(mid) {
            if previous.turn != turn {
                return Err(Error::conflict());
            }
            snap.used = previous.used.clone();
        }
        self.messages.insert(mid.to_owned(), snap);
        Ok(json!({"type":"surface","messageId":mid,"content":content}))
    }
    pub fn authorize(&mut self, action: &Value, origin: &str) -> Result<String> {
        if !bounded(action, 0)
            || action.to_string().len() > 8192
            || !validators()
                .ok_or_else(Error::unavailable)?
                .1
                .is_valid(&json!({"version":"1.0","action":action}))
        {
            return Err(Error::bad());
        }
        let sid = id(&action["surfaceId"])?;
        let cid = id(&action["sourceComponentId"])?;
        let snap = self
            .messages
            .values_mut()
            .find(|m| m.turn == origin && m.live.contains_key(sid))
            .ok_or_else(Error::missing)?;
        let surface = &snap.live[sid];
        let c = surface.components.get(cid).ok_or_else(Error::missing)?;
        let event = &c["action"]["event"];
        if c["component"] != "Button" || event["name"] != action["name"] {
            return Err(Error::forbidden());
        }
        let ctx = if let Some(ctx) = event.get("context") {
            action_context(ctx, &surface.data)?
        } else {
            json!({})
        };
        if action["context"] != ctx {
            return Err(Error::forbidden());
        }
        let fingerprint = digest(
            json!([sid, cid, event["name"], ctx, snap.content])
                .to_string()
                .as_bytes(),
        );
        if snap.used.len() >= 256 || !snap.used.insert(fingerprint.clone()) {
            return Err(Error::conflict());
        }
        Ok(fingerprint)
    }
}
