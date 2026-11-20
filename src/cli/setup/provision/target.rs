use super::{
    Integration::*, Provisioner, Scope, SetupRequest, Transport, digest, editor, safe_path, source,
};
use anyhow::{Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use std::io::Read;
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Target {
    pub root: PathBuf,
    pub file: PathBuf,
    pub keys: Vec<&'static str>,
    pub entry: Value,
    pub append: bool,
    pub is_toml: bool,
    pub export_only: bool,
    pub opencode: bool,
    pub client: &'static str,
}
impl Target {
    pub fn export(&self) -> Value {
        let mut value = if self.append {
            json!([self.entry])
        } else {
            self.entry.clone()
        };
        for key in self.keys.iter().rev() {
            value = json!({(*key):value});
        }
        value
    }
    pub fn edit(&self, raw: Option<&str>) -> Result<String> {
        if self.is_toml {
            editor::toml(raw, &self.entry)
        } else {
            editor::json(raw, &self.keys, &self.entry, self.append)
        }
    }
}

fn jsonc_file(dir: &Path) -> PathBuf {
    let jsonc = dir.join("opencode.jsonc");
    if jsonc.exists() {
        jsonc
    } else {
        dir.join("opencode.json")
    }
}

pub(super) fn describe(p: &Provisioner, req: &SetupRequest) -> Result<Target> {
    let global = req.scope == Scope::Global;
    let root = if global { &p.home } else { &p.project };
    let mcp_url = format!("http://127.0.0.1:{}/mcp", p.mcp_port);
    let binary = env::current_exe()?.canonicalize()?;
    let acp = matches!(req.integration, ZedAcp | JetbrainsAcp | GenericAcp);
    let args = ["serve", if acp { "--acp-stdio" } else { "--mcp-stdio" }];
    let http = req.transport == Transport::Http;
    ensure!(!(http && acp), "ACP requires stdio");
    let local = json!({"command":binary,"args":args});
    let mcp = if http {
        json!({"url":mcp_url})
    } else {
        local.clone()
    };
    let xdg = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| p.home.join(".config"));
    let oc_dir = if global {
        xdg.join("opencode")
    } else {
        p.project.clone()
    };
    let oc_file = jsonc_file(&oc_dir);
    let zed = if global {
        xdg.join("zed/settings.json")
    } else {
        p.project.join(".zed/settings.json")
    };
    let mut export_only = false;
    let (client, file, keys, mut entry) = match req.integration {
        OpencodePlugin => {
            let plugin = env::var_os("OPENAI_PROXY_PLUGIN_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    binary
                        .parent()?
                        .parent()
                        .map(|b| b.join("share/openai-proxy/plugin"))
                })
                .filter(|dir| dir.join("package.json").is_file())
                .or_else(|| {
                    let dir = p.project.join("plugin");
                    dir.join("package.json").is_file().then_some(dir)
                });
            let entry = if let Some(dir) = plugin {
                let meta: Value =
                    serde_json::from_str(&std::fs::read_to_string(dir.join("package.json"))?)?;
                ensure!(
                    meta["name"] == "@prometheus-ags/opencode-codex-proxy"
                        && meta["version"] == "0.2.0",
                    "unsupported plugin package"
                );
                json!(dir.canonicalize()?)
            } else {
                export_only = true;
                json!("@prometheus-ags/opencode-codex-proxy@0.2.0")
            };
            ("opencode", oc_file, vec!["plugins"], entry)
        }
        OpencodeProvider => {
            let profile = crate::auth::setup_profile().ok();
            let models:serde_json::Map<_,_>=crate::model_catalog::CATALOG.iter().filter(|m|profile.is_some_and(|p|m.supports(p)))
              .map(|m|(m.model_id.to_owned(),json!({"name":m.model_id,"capabilities":{"tools":false,"input":["text"],"output":["text"]},
                "limit":{"context":m.context_length_for(profile.unwrap()),"output":m.max_output_tokens}}))).collect();
            let url = p.base_url.clone().unwrap_or_else(|| {
                format!("http://127.0.0.1:{}/api/v1/drivers/codex/v1", p.proxy_port)
            });
            (
                "opencode",
                oc_file,
                vec!["providers", "openai-proxy"],
                json!({"name":"OpenAI Proxy","package":"@opencode/ai/providers/openai-compatible",
                "settings":{"baseURL":url,"apiKey":"{env:PROXY_CONSOLE_TOKEN}"},"models":models}),
            )
        }
        OpencodeMcp => (
            "opencode",
            oc_file,
            vec!["mcp", "servers", "openai-proxy"],
            if http {
                json!({"type":"remote","url":mcp_url})
            } else {
                json!({"type":"local","command":[binary,"serve","--mcp-stdio"]})
            },
        ),
        ClaudeMcp => (
            "claude",
            root.join(if global { ".claude.json" } else { ".mcp.json" }),
            vec!["mcpServers", "openai-proxy"],
            if http {
                json!({"type":"http","url":mcp_url})
            } else {
                json!({"type":"stdio","command":binary,"args":args})
            },
        ),
        CodexMcp => (
            "codex",
            if global {
                env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| p.home.join(".codex"))
                    .join("config.toml")
            } else {
                p.project.join(".codex/config.toml")
            },
            vec!["mcp_servers", "openai-proxy"],
            mcp,
        ),
        ZedMcp => ("zed", zed, vec!["context_servers", "openai-proxy"], mcp),
        VscodeMcp => {
            let dir = if global && cfg!(target_os = "macos") {
                p.home.join("Library/Application Support/Code/User")
            } else if global {
                xdg.join("Code/User")
            } else {
                p.project.join(".vscode")
            };
            (
                "code",
                dir.join("mcp.json"),
                vec!["servers", "openai-proxy"],
                if http {
                    json!({"type":"http","url":mcp_url})
                } else {
                    json!({"type":"stdio","command":binary,"args":args})
                },
            )
        }
        ZedAcp => (
            "zed",
            zed,
            vec!["agent_servers", "openai-proxy"],
            json!({"type":"custom","command":binary,"args":args}),
        ),
        JetbrainsAcp => {
            export_only = !global;
            (
                "idea",
                p.home.join(".jetbrains/acp.json"),
                vec!["agent_servers", "openai-proxy"],
                local,
            )
        }
        GenericAcp | GenericMcp => {
            export_only = true;
            ("generic", root.join("descriptor.json"), vec![], mcp)
        }
    };
    if http && env::var_os("PROXY_MCP_TOKEN").is_some() {
        match req.integration {
            CodexMcp => entry["bearer_token_env_var"] = json!("PROXY_MCP_TOKEN"),
            OpencodeMcp => {
                entry["headers"] = json!({"Authorization":"Bearer {env:PROXY_MCP_TOKEN}"});
                entry["oauth"] = json!(false);
            }
            ClaudeMcp => entry["headers"] = json!({"Authorization":"Bearer ${PROXY_MCP_TOKEN}"}),
            VscodeMcp => {
                entry["headers"] = json!({"Authorization":"Bearer ${env:PROXY_MCP_TOKEN}"})
            }
            ZedMcp | GenericMcp => export_only = true,
            _ => {}
        }
    }
    let approved_root = p.approved_root(req.scope, &file)?;
    Ok(Target {
        root: approved_root,
        file,
        keys,
        entry,
        append: req.integration == OpencodePlugin,
        is_toml: req.integration == CodexMcp,
        opencode: client == "opencode",
        export_only,
        client,
    })
}

pub(super) fn host_version(client: &str) -> Result<String> {
    let paths = env::var_os("PATH").unwrap_or_default();
    let binary = env::split_paths(&paths)
        .filter(|p| p.is_absolute())
        .map(|dir| dir.join(client))
        .find(|p| p.is_file())
        .ok_or_else(|| anyhow!("client executable is unavailable"))?;
    let output = command_output(&binary, &["--version"])?;
    let version = output
        .split_whitespace()
        .map(|s| {
            s.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
                .trim_start_matches('v')
        })
        .find_map(|s| semver::Version::parse(s).ok())
        .ok_or_else(|| anyhow!("unrecognized client version"))?;
    if client == "opencode" {
        ensure!(
            version.major == 2
                && version >= semver::Version::new(2, 3, 0)
                && version.pre.is_empty(),
            "requires stable OpenCode >=2.3.0 and <3.0.0"
        );
    }
    if client == "opencode" {
        for (group, required) in [
            ("plugin", &["list", "add", "check"][..]),
            ("mcp", &["list", "add"][..]),
            ("models", &["models"][..]),
        ] {
            let help = command_output(&binary, &[group, "--help"])?;
            ensure!(
                required.iter().all(|word| help
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|token| token == *word)),
                "required V2 host capability is unavailable"
            );
        }
    }
    Ok(version.to_string())
}

fn command_output(binary: &Path, args: &[&str]) -> Result<String> {
    let mut child = Command::new(binary)
        .args(args)
        .current_dir(env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let output = loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "client did not report a version");
            let mut text = String::new();
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("version unavailable"))?
                .take(16384)
                .read_to_string(&mut text)?;
            break text;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("client version check timed out");
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    Ok(output)
}

pub(super) fn effective_revision(p: &Provisioner, t: &Target) -> Result<String> {
    if !t.opencode {
        return Ok(String::new());
    }
    ensure!(
        [
            "OPENCODE_CONFIG",
            "OPENCODE_CONFIG_CONTENT",
            "OPENCODE_CONFIG_DIR"
        ]
        .iter()
        .all(|key| env::var_os(key).is_none()),
        "an external OpenCode configuration override is active"
    );
    let mut dirs = vec![
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| p.home.join(".config"))
            .join("opencode"),
    ];
    let mut ancestors = p.project.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    dirs.extend(ancestors.iter().map(|dir| dir.to_path_buf()));
    dirs.extend(ancestors.iter().map(|dir| dir.join(".opencode")));
    dirs.push(if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/opencode")
    } else {
        PathBuf::from("/etc/opencode")
    });
    if cfg!(target_os = "macos") {
        let prefs = Path::new("/Library/Managed Preferences");
        let user = p.home.file_name().unwrap_or_default();
        ensure!(
            !prefs.join("ai.opencode.managed.plist").exists()
                && !prefs.join(user).join("ai.opencode.managed.plist").exists(),
            "managed OpenCode preferences require an administrator review"
        );
    }
    let mut revisions = String::new();
    for dir in dirs {
        let candidates = [dir.join("opencode.json"), dir.join("opencode.jsonc")];
        ensure!(
            !(candidates[0].exists() && candidates[1].exists()),
            "both JSON and JSONC OpenCode configurations exist in one layer"
        );
        for path in candidates {
            let Some(raw) = source(&path)? else { continue };
            let config = editor::read(&raw)?;
            ensure!(
                config.get("plugin").is_none()
                    && config.get("provider").is_none()
                    && !config.get("mcp").is_some_and(|m| m
                        .as_object()
                        .is_none_or(|o| o.keys().any(|k| k != "servers"))),
                "legacy or mixed OpenCode configuration at {}",
                path.display()
            );
            let owned = t.keys.iter().try_fold(&config, |node, key| node.get(*key));
            if path != t.file && owned.is_some() && !t.append {
                bail!(
                    "another configuration layer owns the requested setting at {}",
                    path.display()
                );
            }
            revisions.push_str(&digest(path.to_string_lossy().as_bytes()));
            revisions.push_str(&digest(raw.as_bytes()));
        }
    }
    safe_path(&t.root, &t.file)?;
    Ok(digest(revisions.as_bytes()))
}
