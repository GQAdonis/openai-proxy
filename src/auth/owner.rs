use std::{
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant, SystemTime},
};

use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStdin, ChildStdout, Command},
    time::timeout,
};
use tokio_util::codec::{FramedRead, LinesCodec};

use super::store::{Record, Store};

pub(super) enum Outcome {
    Reconcile,
    RetryLater,
    Indeterminate,
    SignInRequired,
    Superseded,
}

/// Acquire before sending an exchange. A process crash after dispatch leaves an
/// opaque receipt, so another proxy process cannot exchange the same grant again.
fn claim(home: &Path, record: &Record) -> std::io::Result<PathBuf> {
    let directory = home.join(".openai-proxy-refresh");
    std::fs::create_dir_all(&directory)?;
    let mut hash = Sha256::new();
    hash.update(b"openai-proxy/managed-refresh/v1\0");
    hash.update(record.auth.access_token.as_deref().unwrap_or_default());
    let path = directory.join(format!("{:x}", hash.finalize()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(b"exchange pending\n")?;
    file.sync_all()?;
    Ok(path)
}

pub(super) fn executable() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("PROXY_CODEX_BINARY") {
        let path = PathBuf::from(configured);
        return (path.is_absolute() && path.is_file()).then_some(path);
    }
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    std::env::var_os("PATH").and_then(|value| {
        std::env::split_paths(&value)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    })
}

const SUPPORTED_CLI: &str = "0.169.0";

#[derive(Clone, PartialEq, Eq)]
struct Binary {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
    #[cfg(unix)]
    inode: u64,
}
impl Binary {
    fn selected() -> Option<Self> {
        let path = executable()?.canonicalize().ok()?;
        let meta = path.metadata().ok()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(unix)]
        if meta.mode() & 0o111 == 0 {
            return None;
        }
        Some(Self {
            path,
            modified: meta.modified().ok(),
            length: meta.len(),
            #[cfg(unix)]
            inode: meta.ino(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Compatibility {
    Available,
    Missing,
    Unsupported,
    Unavailable,
}
impl Compatibility {
    pub fn name(self) -> &'static str {
        match self {
            Self::Available => "supported",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported_version",
            Self::Unavailable => "version_unavailable",
        }
    }
}
#[derive(Default)]
pub(super) struct Runtime(tokio::sync::Mutex<Option<(Binary, Compatibility, Instant)>>);

impl Runtime {
    pub async fn compatibility(&self) -> Compatibility {
        let Some(bin) = Binary::selected() else {
            return Compatibility::Missing;
        };
        let mut cache = self.0.lock().await;
        if let Some((previous, status, when)) = &*cache
            && previous == &bin
            && (*status != Compatibility::Unavailable || when.elapsed() < Duration::from_secs(30))
        {
            return *status;
        }
        let status = check_version(&bin).await;
        *cache = Some((bin, status, Instant::now()));
        status
    }

    async fn verified_binary(&self) -> Option<Binary> {
        if self.compatibility().await != Compatibility::Available {
            return None;
        }
        let cached = self.0.lock().await;
        let (bin, Compatibility::Available, _) = cached.as_ref()? else {
            return None;
        };
        (Binary::selected().as_ref() == Some(bin)).then(|| bin.clone())
    }
}

fn base_command(bin: &Binary, home: &Path, temp: &Path) -> std::io::Result<Command> {
    let mut command = Command::new(&bin.path);
    let mut search = Vec::new();
    // npm distributions may use an env-node launcher next to the selected shim.
    if let Some(location) = executable()
        && let Some(parent) = location.parent()
    {
        search.push(parent.to_path_buf());
    }
    if let Some(parent) = bin.path.parent() {
        search.push(parent.to_path_buf());
    }
    #[cfg(unix)]
    search.extend(
        ["/usr/local/bin", "/usr/bin", "/bin"]
            .into_iter()
            .map(PathBuf::from),
    );
    #[cfg(windows)]
    if let Some(system) = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|s| s.is_absolute())
    {
        search.push(system.join("System32"));
    }
    let path = std::env::join_paths(search).map_err(std::io::Error::other)?;
    command
        .env_clear()
        .env("PATH", path)
        .env("HOME", home)
        .env("CODEX_HOME", home)
        .env("LANG", "C")
        .env("TMPDIR", temp)
        .current_dir(home)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        command
            .env("USERPROFILE", home)
            .env("TEMP", temp)
            .env("TMP", temp);
        if let Some(root) = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|s| s.is_absolute())
        {
            command.env("SystemRoot", root);
        }
    }
    Ok(command)
}

fn private_scratch() -> std::io::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("openai-proxy-codex-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir()
}

async fn check_version(bin: &Binary) -> Compatibility {
    let Ok(scratch) = private_scratch() else {
        return Compatibility::Unavailable;
    };
    let Ok(mut command) = base_command(bin, scratch.path(), scratch.path()) else {
        return Compatibility::Unavailable;
    };
    let Ok(mut child) = command.arg("--version").spawn() else {
        return Compatibility::Unavailable;
    };
    let Some(output) = child.stdout.take() else {
        return Compatibility::Unavailable;
    };
    let result = timeout(Duration::from_secs(3), async {
        let mut bytes = Vec::with_capacity(128);
        output.take(129).read_to_end(&mut bytes).await?;
        if bytes.len() > 128 {
            return Err(std::io::Error::other("unexpected version response"));
        }
        let code = child.wait().await?;
        if !code.success() {
            return Err(std::io::Error::other("version unavailable"));
        }
        Ok(bytes)
    })
    .await;
    match result {
        Ok(Ok(bytes))
            if std::str::from_utf8(&bytes)
                .ok()
                .is_some_and(|v| v.trim().strip_prefix("codex-cli ") == Some(SUPPORTED_CLI)) =>
        {
            Compatibility::Available
        }
        Ok(Ok(_)) => Compatibility::Unsupported,
        _ => Compatibility::Unavailable,
    }
}

#[cfg(target_os = "linux")]
fn local_bus(store: &Store) -> Option<(PathBuf, String)> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let run = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)?;
    if !run.is_absolute() || run.canonicalize().ok().as_ref() != Some(&run) {
        return None;
    }
    let (dir, home) = (run.metadata().ok()?, store.home().metadata().ok()?);
    let bus = run.join("bus");
    if !dir.is_dir()
        || dir.uid() != home.uid()
        || dir.mode() & 0o077 != 0
        || !bus
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_socket() && m.uid() == dir.uid())
    {
        return None;
    }
    let address = format!("unix:path={}", bus.display());
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS")
        .is_some_and(|given| given.to_str() != Some(address.as_str()))
    {
        return None;
    }
    Some((run, address))
}

#[cfg(target_os = "linux")]
fn needs_broker(store: &Store) -> bool {
    let config = std::fs::read_to_string(store.home().join("config.toml"))
        .ok()
        .and_then(|doc| toml::from_str::<toml::Table>(&doc).ok());
    config
        .as_ref()
        .and_then(|t| t.get("cli_auth_credentials_store"))
        .and_then(|v| v.as_str())
        .is_some_and(|v| matches!(v, "keyring" | "auto"))
}

pub(super) fn broker_available(store: &Store) -> bool {
    #[cfg(target_os = "linux")]
    {
        !needs_broker(store) || local_bus(store).is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = store;
        true
    }
}

async fn write_request(input: &mut ChildStdin, value: Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(&value)?;
    bytes.push(b'\n');
    input.write_all(&bytes).await?;
    input.flush().await
}

async fn reply(input: &mut FramedRead<ChildStdout, LinesCodec>, id: u64) -> std::io::Result<Value> {
    for _ in 0..128 {
        let line = input
            .next()
            .await
            .ok_or_else(|| std::io::Error::new(ErrorKind::UnexpectedEof, "Codex helper closed"))?
            .map_err(std::io::Error::other)?;
        let result: Value = serde_json::from_str(&line)?;
        if result["id"].as_u64() == Some(id) && result.get("method").is_none() {
            return Ok(result);
        }
    }
    Err(std::io::Error::other("too many unrelated helper frames"))
}

pub(super) async fn exchange(
    runtime: &Runtime,
    store: &Store,
    record: &Record,
    current: impl Fn() -> bool,
) -> Outcome {
    let Some(bin) = runtime.verified_binary().await else {
        return Outcome::RetryLater;
    };
    if !current() {
        return Outcome::Superseded;
    }
    let Ok(scratch) = private_scratch() else {
        return Outcome::RetryLater;
    };
    let Ok(mut command) = base_command(&bin, store.home(), scratch.path()) else {
        return Outcome::RetryLater;
    };
    #[cfg(target_os = "linux")]
    if needs_broker(store) {
        let Some((run, bus)) = local_bus(store) else {
            return Outcome::RetryLater;
        };
        command
            .env("XDG_RUNTIME_DIR", run)
            .env("DBUS_SESSION_BUS_ADDRESS", bus);
    }
    let mut child = match command.arg("app-server").stdin(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(_) => return Outcome::RetryLater,
    };
    let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
        return Outcome::RetryLater;
    };
    let mut output = FramedRead::new(output, LinesCodec::new_with_max_length(256 * 1024));
    let handshake = async {
        write_request(&mut input, json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":"openai_proxy","version":env!("CARGO_PKG_VERSION")},"capabilities":null
        }})).await?;
        let initialized = reply(&mut output, 1).await?;
        let reported = initialized["result"]["codexHome"]
            .as_str()
            .and_then(|p| Path::new(p).canonicalize().ok());
        let expected = store.home().canonicalize()?;
        if reported.as_ref() != Some(&expected) {
            return Err(std::io::Error::other("Codex helper home mismatch"));
        }
        write_request(&mut input, json!({"method":"initialized","params":{}})).await
    };
    if !matches!(timeout(Duration::from_secs(5), handshake).await, Ok(Ok(()))) {
        return Outcome::RetryLater;
    }
    // Only allow the writer that owns the exact committed generation to proceed.
    if !current() || store.read().ok().as_ref() != Some(record) {
        return Outcome::Superseded;
    }
    let claim = match claim(store.home(), record) {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Outcome::Indeterminate,
        Err(_) => return Outcome::RetryLater,
    };
    // Between the final check and dispatch, native Codex performs the
    // account-guarded compare-and-save against exactly this store.
    if !current() || store.read().ok().as_ref() != Some(record) {
        let _ = std::fs::remove_file(claim);
        return Outcome::Superseded;
    }
    let result = timeout(Duration::from_secs(15), async {
        write_request(
            &mut input,
            json!({"id":2,"method":"account/read","params":{"refreshToken":true}}),
        )
        .await?;
        reply(&mut output, 2).await
    })
    .await;
    let outcome = match result {
        Ok(Ok(value)) if value.get("result").is_some() => Outcome::Reconcile,
        Ok(Ok(value)) => {
            let error = value["error"].to_string().to_ascii_lowercase();
            if [
                "invalid_grant",
                "refresh_token_reused",
                "refresh_token_revoked",
                "refresh_token_expired",
            ]
            .iter()
            .any(|code| error.contains(code))
            {
                Outcome::SignInRequired
            } else {
                Outcome::Indeterminate
            }
        }
        _ => Outcome::Indeterminate,
    };
    let _ = child.kill().await;
    // A demonstrably newer commit allows a future exchange of that new grant.
    // Keeping the old receipt prevents replay if a stale backup is restored.
    outcome
}
