/// Configuration loader for openai-proxy.
///
/// Resolution order (later wins):
///   defaults → `$XDG_CONFIG_HOME/oproxy/config.toml` → `--config <path>` → env vars → CLI flags
///
/// The config file is optional — all fields have sane defaults and the proxy
/// continues to work with env vars alone for backward compatibility.
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Returns `$XDG_CONFIG_HOME/oproxy` (or `~/.config/oproxy` as fallback).
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("oproxy")
}

/// Returns `$XDG_DATA_HOME/oproxy` (or `~/.local/share/oproxy` as fallback).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("oproxy")
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8181
}
fn default_wire_api() -> String {
    "responses".to_string()
}
fn default_max_injected() -> usize {
    3
}
fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BackendConfig {
    /// "responses" (default) or "chat"
    #[serde(default = "default_wire_api")]
    pub wire_api: String,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            wire_api: default_wire_api(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SkillsConfig {
    pub dirs: Vec<String>,
    #[serde(default = "default_max_injected")]
    pub max_injected: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct McpConfig {
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct HooksConfig {
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub db_path: String,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: String::new(),
            embedding_model: default_embedding_model(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ModesConfig {
    pub a2a: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ProxyConfig {
    pub server: ServerConfig,
    pub backend: BackendConfig,
    pub skills: SkillsConfig,
    pub mcp: McpConfig,
    pub hooks: HooksConfig,
    pub memory: MemoryConfig,
    pub modes: ModesConfig,
}

impl ProxyConfig {
    /// Load config from `path` (if provided) or the XDG default location.
    /// Missing config file is not an error — defaults are returned.
    /// Returns errors only for parse failures.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let resolved = path
            .map(PathBuf::from)
            .unwrap_or_else(|| config_dir().join("config.toml"));

        if !resolved.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(&resolved)
            .map_err(|e| anyhow::anyhow!("failed to read config {}: {e}", resolved.display()))?;

        let cfg: Self = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("failed to parse config {}: {e}", resolved.display()))?;

        Ok(cfg)
    }

    /// Apply env var overrides. Env vars win over the config file but lose to
    /// explicit CLI flags (which are applied by the caller after this).
    pub fn apply_env(&mut self) {
        self.apply_env_from(|key| std::env::var(key).ok());
    }

    fn apply_env_from<F>(&mut self, mut var: F)
    where
        F: FnMut(&str) -> Option<String>,
    {
        if let Some(h) = var("HOST") {
            self.server.host = h;
        }
        if let Some(p) = var("PORT") {
            if let Ok(n) = p.parse() {
                self.server.port = n;
            }
        }
        if let Some(w) = var("CODEX_WIRE_API") {
            self.backend.wire_api = w;
        }
        if let Some(dirs) = var("PROXY_SKILLS_DIRS") {
            let extra: Vec<String> = dirs
                .split(':')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            self.skills.dirs.extend(extra);
        }
        if let Some(m) = var("PROXY_SKILLS_MAX") {
            if let Ok(n) = m.parse() {
                self.skills.max_injected = n;
            }
        }
        if let Some(p) = var("PROXY_HOOKS_CONFIG") {
            self.hooks.config_path = Some(p);
        }
    }
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn load_missing_file_returns_defaults() {
        let cfg = ProxyConfig::load(Some(Path::new("/nonexistent/path/config.toml"))).unwrap();
        assert_eq!(cfg.server.port, 8181);
        assert_eq!(cfg.backend.wire_api, "responses");
        assert!(!cfg.memory.enabled);
    }

    #[test]
    fn load_valid_toml() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[server]\nport = 9090\n[backend]\nwire_api = \"chat\"").unwrap();
        let cfg = ProxyConfig::load(Some(f.path())).unwrap();
        assert_eq!(cfg.server.port, 9090);
        assert_eq!(cfg.backend.wire_api, "chat");
    }

    #[test]
    fn env_var_overrides_config() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[server]\nport = 9090").unwrap();
        let mut cfg = ProxyConfig::load(Some(f.path())).unwrap();
        cfg.apply_env_from(|key| (key == "PORT").then(|| "7777".to_string()));
        assert_eq!(cfg.server.port, 7777);
    }

    #[test]
    fn tilde_expansion_works() {
        let expanded = expand_tilde("~/foo/bar");
        let home = dirs::home_dir().unwrap();
        assert_eq!(expanded, home.join("foo/bar"));
    }

    #[test]
    fn tilde_expansion_no_op_for_absolute() {
        let path = "/absolute/path";
        assert_eq!(expand_tilde(path), PathBuf::from(path));
    }
}
