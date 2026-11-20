use std::{
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::Value;

use crate::codex::{BackendProfile, CodexAuth, codex_keyring_account};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Managed,
    External,
    ApiKey,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
            Self::ApiKey => "api_key",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Record {
    pub auth: CodexAuth,
    pub mode: Mode,
    pub expiry: Option<u64>,
    pub source: &'static str,
}

#[derive(Clone, Copy)]
pub(super) enum ReadError {
    Missing,
    Partial,
    Unavailable,
    Unsupported(&'static str),
}

#[derive(Clone)]
pub(super) struct Store {
    pub path: PathBuf,
    pub native: bool,
    pub environment_key: Option<String>,
    pub wire_chat: bool,
    pub destination: Option<(String, BackendProfile)>,
}

impl Store {
    pub fn process(
        path: Option<PathBuf>,
        wire_api: &str,
        destination: Option<String>,
    ) -> anyhow::Result<Self> {
        let path = path.or_else(|| env::var_os("CODEX_AUTH_PATH").map(PathBuf::from));
        let environment = match env::var("PROXY_CHAT_AUTH").as_deref() {
            Ok("api_key") => true,
            Ok("codex") | Err(_) => false,
            Ok(_) => anyhow::bail!("PROXY_CHAT_AUTH must be codex or api_key"),
        };
        let path = match path {
            Some(path) => path,
            None => env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|p| p.join(".codex")))
                .ok_or_else(|| {
                    anyhow::anyhow!("set CODEX_HOME to the selected credential directory")
                })?
                .join("auth.json"),
        };
        let mut store = Self::file(path);
        if environment {
            store.native = false;
            store.environment_key = Some(env::var("OPENAI_API_KEY").unwrap_or_default());
        }
        store.wire_chat = wire_api.eq_ignore_ascii_case("chat");
        if let Some(url) = destination {
            let parsed = reqwest::Url::parse(&url)?;
            anyhow::ensure!(
                parsed.username().is_empty()
                    && parsed.password().is_none()
                    && parsed.query().is_none()
                    && parsed.fragment().is_none(),
                "backend URL must be a credential-free endpoint"
            );
            let profile = match url.as_str() {
                crate::codex::CODEX_BACKEND_URL => BackendProfile::ChatGptCodex,
                crate::codex::OPENAI_RESPONSES_URL => BackendProfile::OpenAiResponses,
                crate::codex::OPENAI_CHAT_URL => BackendProfile::OpenAiChatCompletions,
                _ => {
                    let loopback = parsed.host_str().is_some_and(|s| {
                        s == "localhost"
                            || s.trim_matches(['[', ']'])
                                .parse::<std::net::IpAddr>()
                                .is_ok_and(|ip| ip.is_loopback())
                    });
                    anyhow::ensure!(
                        parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback),
                        "custom upstream requires HTTPS or loopback HTTP"
                    );
                    match env::var("PROXY_BACKEND_CREDENTIAL_CLASS").as_deref() {
                        Ok("subscription") if loopback => BackendProfile::ChatGptCodex,
                        Ok("api_key") if store.wire_chat => BackendProfile::OpenAiChatCompletions,
                        Ok("api_key") => BackendProfile::OpenAiResponses,
                        _ => anyhow::bail!(
                            "authorize the custom backend's credential class explicitly"
                        ),
                    }
                }
            };
            store.destination = Some((url, profile));
        }
        Ok(store)
    }

    pub fn file(path: PathBuf) -> Self {
        let path = std::path::absolute(&path).unwrap_or(path);
        Self {
            native: path.file_name().is_some_and(|n| n == "auth.json"),
            path,
            environment_key: None,
            wire_chat: false,
            destination: None,
        }
    }

    pub fn home(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    pub fn route(&self, record: &Record) -> Option<(String, BackendProfile)> {
        let subscription = record.mode != Mode::ApiKey;
        if let Some((url, profile)) = &self.destination {
            return (subscription == (*profile == BackendProfile::ChatGptCodex))
                .then(|| (url.clone(), *profile));
        }
        let profile = if subscription {
            BackendProfile::ChatGptCodex
        } else if self.wire_chat {
            BackendProfile::OpenAiChatCompletions
        } else {
            BackendProfile::OpenAiResponses
        };
        Some((profile.backend_url().into(), profile))
    }

    pub fn read(&self) -> Result<Record, ReadError> {
        if let Some(key) = &self.environment_key {
            if key.trim().is_empty() {
                return Err(ReadError::Missing);
            }
            return Ok(Record {
                auth: CodexAuth {
                    access_token: None,
                    account_id: None,
                    api_key: Some(key.clone()),
                },
                mode: Mode::ApiKey,
                expiry: None,
                source: "environment",
            });
        }
        if !super::topology_ok(&self.path) {
            return Err(ReadError::Unsupported("single_file_mount"));
        }
        if self.native {
            let config = match std::fs::read_to_string(self.home().join("config.toml")) {
                Ok(s) => toml::from_str::<toml::Table>(&s).map_err(|_| ReadError::Unavailable)?,
                Err(e) if e.kind() == ErrorKind::NotFound => toml::Table::new(),
                Err(_) => return Err(ReadError::Unavailable),
            };
            let mode = match config.get("cli_auth_credentials_store") {
                Some(value) => value
                    .as_str()
                    .ok_or(ReadError::Unsupported("store_configuration"))?,
                None => "file",
            };
            match mode {
                "file" => {}
                mode @ ("keyring" | "auto") => {
                    let encrypted = config
                        .get("features")
                        .and_then(|v| v.get("secret_auth_storage"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(cfg!(windows));
                    if encrypted {
                        return Err(ReadError::Unsupported("encrypted_keyring"));
                    }
                    let entry =
                        keyring::Entry::new("Codex Auth", &codex_keyring_account(self.home()))
                            .map_err(|_| ReadError::Unavailable)?;
                    match entry.get_password() {
                        Ok(raw) => return parse(&raw, "keyring"),
                        Err(keyring::Error::NoEntry) if mode == "auto" => {}
                        Err(keyring::Error::NoEntry) => return Err(ReadError::Missing),
                        Err(_) => return Err(ReadError::Unavailable),
                    }
                }
                "ephemeral" => return Err(ReadError::Unsupported("ephemeral")),
                _ => return Err(ReadError::Unsupported("store_configuration")),
            }
        }
        let text = std::fs::read_to_string(&self.path).map_err(|e| match e.kind() {
            ErrorKind::NotFound => ReadError::Missing,
            _ => ReadError::Unavailable,
        })?;
        parse(&text, if self.native { "file" } else { "custom_file" })
    }
}

#[derive(Deserialize, Default)]
struct Tokens {
    access_token: Option<String>,
    account_id: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct Document {
    #[serde(flatten)]
    legacy: Tokens,
    tokens: Option<Tokens>,
    auth_mode: Option<String>,
    #[serde(alias = "OPENAI_API_KEY")]
    api_key: Option<String>,
}

fn parse(text: &str, source: &'static str) -> Result<Record, ReadError> {
    let doc: Document = serde_json::from_str(text).map_err(|_| ReadError::Partial)?;
    let tokens = doc.tokens.unwrap_or(doc.legacy);
    let kind = doc.auth_mode.as_deref();
    let key = doc.api_key.filter(|s| !s.trim().is_empty());
    let token = tokens.access_token.filter(|s| !s.trim().is_empty());
    let account = tokens.account_id.filter(|s| !s.trim().is_empty());
    if kind == Some("apikey")
        || (kind.is_none() && token.is_none() && account.is_none() && key.is_some())
    {
        let api_key = key.ok_or(ReadError::Partial)?;
        return Ok(Record {
            mode: Mode::ApiKey,
            auth: CodexAuth {
                api_key: Some(api_key),
                account_id: None,
                access_token: None,
            },
            expiry: None,
            source,
        });
    }
    let mode = match kind {
        None | Some("chatgpt") => {
            if tokens.refresh_token.is_some_and(|s| !s.is_empty()) {
                Mode::Managed
            } else {
                Mode::External
            }
        }
        Some("chatgptAuthTokens") => Mode::External,
        _ => return Err(ReadError::Unsupported("credential_mode")),
    };
    let (Some(access_token), Some(account_id)) = (token, account) else {
        return Err(ReadError::Partial);
    };
    let expiry = access_token
        .split('.')
        .nth(1)
        .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["exp"].as_u64());
    Ok(Record {
        auth: CodexAuth {
            access_token: Some(access_token),
            account_id: Some(account_id),
            api_key: None,
        },
        mode,
        expiry,
        source,
    })
}
