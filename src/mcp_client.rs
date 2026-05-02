/// MCP tool schema passthrough.
///
/// Loads tool definitions from a TOML config and injects them into every
/// proxied request so the model knows what MCP tools are available.
/// No MCP servers are spawned — this is schema passthrough only.
use std::path::Path;

use serde::{Deserialize, Serialize};

/// An OpenAI-compatible tool definition (function schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolSchema {
    #[serde(rename = "type", default = "function_type")]
    pub tool_type: String,
    pub function: McpFunctionDef,
}

fn function_type() -> String { "function".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpFunctionDef {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

// ── Config file structs ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct McpConfig {
    #[serde(default)]
    tool: Vec<ToolEntry>,
    sources: Option<McpSources>,
}

#[derive(Debug, Deserialize)]
struct ToolEntry {
    name: String,
    description: String,
    input_schema: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct McpSources {
    claude_code: Option<String>,
}

// ── Loader ───────────────────────────────────────────────────────────────────

/// Load tool schemas from a TOML config file.
/// Returns an empty vec if the file is missing or has no tools.
pub fn load_mcp_tools(config_path: &Path) -> Vec<McpToolSchema> {
    if !config_path.exists() {
        return Vec::new();
    }

    let raw = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %config_path.display(), error = %e, "cannot read mcp config");
            return Vec::new();
        }
    };

    let cfg: McpConfig = match toml::from_str(&raw) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %config_path.display(), error = %e, "cannot parse mcp config");
            return Vec::new();
        }
    };

    let tools: Vec<McpToolSchema> = cfg.tool.into_iter().map(|t| McpToolSchema {
        tool_type: "function".to_string(),
        function: McpFunctionDef {
            name: t.name,
            description: t.description,
            parameters: t.input_schema,
        },
    }).collect();

    // Import tool names from claude_code sources (schema must be declared inline).
    if let Some(sources) = cfg.sources {
        if let Some(path_raw) = sources.claude_code {
            let path = crate::config::expand_tilde(&path_raw);
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
                        for (name, _) in servers {
                            // Only add if not already declared inline.
                            if !tools.iter().any(|t| &t.function.name == name) {
                                tracing::debug!(name = %name, "imported mcp server name from claude_code config (no schema)");
                            }
                        }
                    }
                }
            }
        }
    }

    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn loads_inline_tool() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"
[[tool]]
name = "read_file"
description = "Read a file"
[tool.input_schema]
type = "object"
"#).unwrap();
        let tools = load_mcp_tools(f.path());
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "read_file");
    }

    #[test]
    fn returns_empty_for_missing_file() {
        let tools = load_mcp_tools(Path::new("/nonexistent/mcp.toml"));
        assert!(tools.is_empty());
    }

    #[test]
    fn tool_serializes_as_openai_format() {
        let tool = McpToolSchema {
            tool_type: "function".to_string(),
            function: McpFunctionDef {
                name: "test_fn".to_string(),
                description: "A test".to_string(),
                parameters: Some(serde_json::json!({"type": "object"})),
            },
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"name\":\"test_fn\""));
    }
}
