use anyhow::{Result, anyhow, bail, ensure};
use jsonc_parser::{
    ast::{Object, Value as JsoncValue},
    common::Ranged,
    parse_to_ast,
};
use serde_json::{Value, json};
use std::collections::HashSet;
use toml_edit::{DocumentMut, Item, Table};

fn check_keys(node: &JsoncValue<'_>) -> Result<()> {
    if let Some(obj) = node.as_object() {
        let mut names = HashSet::new();
        for prop in &obj.properties {
            ensure!(
                names.insert(prop.name.as_str()),
                "duplicate property in configuration"
            );
            check_keys(&prop.value)?;
        }
    } else if let Some(array) = node.as_array() {
        for value in &array.elements {
            check_keys(value)?;
        }
    }
    Ok(())
}

fn parse(raw: &str) -> Result<Object<'_>> {
    let node = parse_to_ast(raw, &Default::default(), &Default::default())
        .map_err(|_| anyhow!("invalid JSONC"))?
        .value
        .ok_or_else(|| anyhow!("empty JSONC document"))?;
    check_keys(&node)?;
    match node {
        JsoncValue::Object(object) => Ok(object),
        _ => bail!("configuration must be an object"),
    }
}

pub(super) fn read(raw: &str) -> Result<Value> {
    Ok(JsoncValue::Object(parse(raw)?).into())
}

// Insert immediately after the last member (or opening delimiter). Keeping every
// input byte lets undo remove one span without storing any existing settings.
fn insert(raw: &str, opening: usize, last: Option<usize>, member: &str) -> String {
    let mut after = raw.to_owned();
    let (offset, separator) = last.map_or((opening + 1, ""), |end| (end, ", "));
    after.insert_str(offset, &format!("{separator}{member}"));
    after
}

pub(super) fn json(
    raw: Option<&str>,
    keys: &[&str],
    entry: &Value,
    append: bool,
) -> Result<String> {
    ensure!(!keys.is_empty(), "missing owned key");
    let raw = raw.unwrap_or("{}\n");
    let root = parse(raw)?;
    let mut object = &root;
    for (index, key) in keys.iter().enumerate() {
        let Some(prop) = object.get(key) else {
            let mut value = if append {
                json!([entry])
            } else {
                entry.clone()
            };
            for child in keys[index + 1..].iter().rev() {
                value = json!({(*child): value});
            }
            let member = format!("{}: {}", serde_json::to_string(key)?, value);
            return Ok(insert(
                raw,
                object.start(),
                object.properties.last().map(Ranged::end),
                &member,
            ));
        };
        if index + 1 < keys.len() {
            object = prop
                .value
                .as_object()
                .ok_or_else(|| anyhow!("existing setting is not an object"))?;
        } else if append {
            let items = prop
                .value
                .as_array()
                .ok_or_else(|| anyhow!("plugins must be an array"))?;
            if items
                .elements
                .iter()
                .any(|item| Value::from(item.clone()) == *entry)
            {
                return Ok(raw.to_owned());
            }
            return Ok(insert(
                raw,
                items.start(),
                items.elements.last().map(Ranged::end),
                &entry.to_string(),
            ));
        } else {
            ensure!(
                Value::from(prop.value.clone()) == *entry,
                "existing user setting differs from proposed entry"
            );
            return Ok(raw.to_owned());
        }
    }
    unreachable!("a nonempty property path always resolves at its final key")
}

pub(super) fn toml(raw: Option<&str>, entry: &Value) -> Result<String> {
    let mut document = raw
        .unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|_| anyhow!("invalid TOML file"))?;
    let servers = document
        .as_table_mut()
        .entry("mcp_servers")
        .or_insert(Item::Table(Table::new()));
    let servers = servers
        .as_table_like_mut()
        .ok_or_else(|| anyhow!("mcp_servers must be a table"))?;
    let mut owned = Table::new();
    for (key, value) in entry
        .as_object()
        .ok_or_else(|| anyhow!("invalid server descriptor"))?
    {
        let item = match value {
            Value::String(text) => toml_edit::value(text.as_str()),
            Value::Array(values) => {
                let mut array = toml_edit::Array::new();
                for v in values {
                    array.push(v.as_str().ok_or_else(|| anyhow!("invalid argument"))?);
                }
                toml_edit::value(array)
            }
            _ => bail!("unsupported server descriptor field"),
        };
        owned.insert(key, item);
    }
    if servers.contains_key("openai-proxy") {
        let parsed: toml::Value = raw
            .unwrap_or("")
            .parse()
            .map_err(|_| anyhow!("invalid TOML"))?;
        let existing = serde_json::to_value(&parsed["mcp_servers"]["openai-proxy"])?;
        ensure!(
            existing == *entry,
            "existing user setting differs from proposed entry"
        );
    } else {
        servers.insert("openai-proxy", Item::Table(owned));
    }
    Ok(document.to_string())
}
