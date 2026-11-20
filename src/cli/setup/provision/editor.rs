use anyhow::{Result, anyhow, bail, ensure};
use jsonc_parser::{
    ParseOptions,
    cst::{CstInputValue, CstNode, CstObject, CstRootNode},
};
use serde_json::Value;
use std::collections::HashSet;
use toml_edit::{DocumentMut, Item, Table};

fn check_node(node: CstNode) -> Result<()> {
    if let Some(obj) = node.as_object() {
        let mut names = HashSet::new();
        for prop in obj.properties() {
            let name = prop
                .decoded_name()
                .ok_or_else(|| anyhow!("invalid property name"))?;
            ensure!(names.insert(name), "duplicate property in configuration");
            check_node(
                prop.value()
                    .ok_or_else(|| anyhow!("missing property value"))?,
            )?;
        }
    } else if let Some(array) = node.as_array() {
        for value in array.elements() {
            check_node(value)?;
        }
    }
    Ok(())
}

fn parse(raw: &str) -> Result<(CstRootNode, CstObject)> {
    let root =
        CstRootNode::parse(raw, &ParseOptions::default()).map_err(|_| anyhow!("invalid JSONC"))?;
    check_node(
        root.value()
            .ok_or_else(|| anyhow!("empty JSONC document"))?,
    )?;
    let obj = root
        .object_value()
        .ok_or_else(|| anyhow!("configuration must be an object"))?;
    Ok((root, obj))
}

pub(super) fn read(raw: &str) -> Result<Value> {
    let (_, object) = parse(raw)?;
    object
        .to_serde_value()
        .ok_or_else(|| anyhow!("invalid JSONC document"))
}

fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(*b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(input).collect()),
        Value::Object(map) => {
            CstInputValue::Object(map.iter().map(|(k, v)| (k.clone(), input(v))).collect())
        }
    }
}

pub(super) fn json(
    raw: Option<&str>,
    keys: &[&str],
    entry: &Value,
    append: bool,
) -> Result<String> {
    let (root, mut object) = parse(raw.unwrap_or("{}\n"))?;
    let (key, parents) = keys
        .split_last()
        .ok_or_else(|| anyhow!("missing owned key"))?;
    for parent in parents {
        object = object
            .object_value_or_create(parent)
            .ok_or_else(|| anyhow!("existing setting is not an object"))?;
    }
    if append {
        let array = object
            .array_value_or_create(key)
            .ok_or_else(|| anyhow!("plugins must be an array"))?;
        let value = array
            .to_serde_value()
            .ok_or_else(|| anyhow!("invalid plugin array"))?;
        if !value.as_array().is_some_and(|items| items.contains(entry)) {
            array.append(input(entry));
        }
    } else if let Some(prop) = object.get(key) {
        ensure!(
            prop.to_serde_value().as_ref() == Some(entry),
            "existing user setting differs from proposed entry"
        );
    } else {
        object.append(key, input(entry));
    }
    Ok(root.to_string())
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
