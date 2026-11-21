use super::{
    Console, Error, Result,
    access::{self, Actor},
    conversations::{self, Conversation},
};
use futures::StreamExt;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::{ChildStdin, Command},
    sync::mpsc,
    time::{Instant, timeout},
};
use tokio_util::codec::{FramedRead, LinesCodec};

fn binary(env: &str, name: &str) -> Option<PathBuf> {
    if let Some(given) = std::env::var_os(env) {
        let path = PathBuf::from(given);
        return (path.is_absolute() && path.is_file()).then_some(path);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .filter(|p| p.is_absolute())
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}
fn entry() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PROXY_SDK_WORKER") {
        let p = PathBuf::from(p);
        return (p.is_absolute() && p.is_file()).then_some(p);
    }
    let bin = std::env::current_exe().ok()?;
    let p = bin.parent()?.join("../lib/openai-proxy/workers/main.mjs");
    p.is_file().then_some(p)
}
pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && model
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_./:".contains(&c))
        && !model.contains("..")
}
fn provider_target(a: &Actor) -> Option<reqwest::Url> {
    let p = a.provider.as_ref()?;
    let id = p["id"].as_str()?;
    if id.is_empty()
        || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || matches!(id, "codex" | "openai-proxy")
    {
        return None;
    }
    let url = reqwest::Url::parse(p["baseURL"].as_str()?).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    if url.scheme() != "https"
        && !(url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    {
        return None;
    }
    Some(url)
}
pub async fn availability(a: &Actor, driver: &str) -> (bool, &'static str) {
    let Some(main) = entry() else {
        return (false, "worker_not_installed");
    };
    if binary("PROXY_SDK_BUN", "bun").is_none() {
        return (false, "bun_unavailable");
    }
    let (package, version) = match driver {
        "codex" => {
            if binary("PROXY_CODEX_BINARY", "codex").is_none()
                || a.credentials.codex_home().is_none()
                || !a.credentials.native_cli_compatible().await
            {
                return (false, "codex_runtime_or_home_unavailable");
            }
            ("@openai/codex-sdk", "0.169.0")
        }
        "opencode" => {
            if provider_target(a).is_none() {
                return (false, "scoped_provider_required");
            }
            ("@opencode/sdk", "2.3.0")
        }
        _ => return (false, "unsupported_driver"),
    };
    let manifest = main
        .parent()
        .unwrap_or(Path::new(""))
        .join("node_modules")
        .join(package)
        .join("package.json");
    let meta = std::fs::read(manifest)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    if !meta.is_some_and(|m| m["name"] == package && m["version"] == version) {
        return (false, "sdk_missing_or_incompatible");
    }
    (true, "installed")
}
async fn send(w: &mut ChildStdin, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| Error::bad())?;
    bytes.push(b'\n');
    timeout(Duration::from_secs(3), w.write_all(&bytes))
        .await
        .map_err(|_| Error::unavailable())?
        .map_err(|_| Error::unavailable())
}
async fn endpoint_safe(url: &reqwest::Url, s: &Console) -> Result<()> {
    let port = url.port_or_known_default().ok_or_else(Error::bad)?;
    let name = url
        .host_str()
        .ok_or_else(Error::bad)?
        .trim_matches(['[', ']']);
    let addresses = timeout(
        Duration::from_secs(3),
        tokio::net::lookup_host((name, port)),
    )
    .await
    .map_err(|_| Error::unavailable())?
    .map_err(|_| Error::unavailable())?;
    let same_port = port == s.app.runtime.config.server.port;
    let advertised = s.app.bind_addr.parse::<std::net::SocketAddr>().ok();
    let public_origin = std::env::var("PROXY_CONSOLE_ORIGIN")
        .ok()
        .and_then(|u| reqwest::Url::parse(&u).ok());
    if public_origin.is_some_and(|u| u.origin() == url.origin()) {
        return Err(Error::conflict());
    }
    for addr in addresses {
        if same_port
            && (addr.ip().is_loopback()
                || advertised.is_some_and(|server| server.ip() == addr.ip())
                || std::net::TcpListener::bind((addr.ip(), 0)).is_ok())
        {
            return Err(Error::conflict());
        }
    }
    Ok(())
}
struct ProcessGroup(Option<u32>);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // The child has its own group. Stop SDK descendants if cancellation cannot drain IPC.
        #[cfg(unix)]
        if let Some(pid) = self.0.take() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}
fn log_codex_metric(id: &str, frame: &Value) {
    let stage = frame["stage"].as_str().unwrap_or_default();
    let operation = frame["operation"].as_str().unwrap_or_default();
    if !matches!(
        (stage, operation),
        (
            "startup",
            "thread_start_create_thread" | "thread_start_total"
        ) | ("auth_selection", "model" | "all")
            | ("auth_storage", "load" | "save" | "delete" | "all")
            | (
                "auth_refresh_persist" | "model_cache_load" | "model_fetch",
                "all"
            )
    ) {
        return;
    }
    if let (Some(count @ 1..=100_000), Some(duration_ms)) =
        (frame["count"].as_u64(), frame["durationMs"].as_f64())
        && (0.0..=3_600_000.0).contains(&duration_ms)
    {
        tracing::info!(
            turn_id = id,
            stage,
            operation,
            count,
            duration_ms,
            "Codex local duration"
        );
    }
}

async fn supervise(
    s: &Console,
    a: &Actor,
    c: &Conversation,
    id: &str,
    prompt: String,
    mut control: mpsc::Receiver<Value>,
) -> Result<bool> {
    let preparing_since = Instant::now();
    if !availability(a, &c.driver).await.0 {
        return Err(Error::unavailable());
    }
    if conversations::binding(a, &c.driver).await.as_deref() != Some(&c.binding) {
        return Err(Error::forbidden());
    }
    let main = entry().ok_or_else(Error::unavailable)?;
    let root = s
        .root
        .join("workers")
        .join(access::digest(a.id.as_bytes()))
        .join(&c.id);
    access::private_dir(&root).map_err(|_| Error::storage())?;
    let mut start = json!({"v":1,"type":"start","workspace":c.workspace,"nativeId":c.native_id,"prompt":prompt,"model":c.model,"database":root.join("sessions.sqlite")});
    let local_metrics = c.driver == "codex"
        && !s.access.shared
        && std::env::var("PROXY_CODEX_LOCAL_METRICS").as_deref() == Ok("1")
        && s.app
            .bind_addr
            .parse::<std::net::SocketAddr>()
            .is_ok_and(|addr| addr.ip().is_loopback());
    if c.driver == "codex" {
        let auth_started_at = Instant::now();
        let snapshot = a.credentials.snapshot().await;
        if local_metrics {
            start["localMetrics"] = Value::Bool(true);
            tracing::info!(
                turn_id = id,
                stage = "proxy_auth",
                operation = "snapshot",
                count = 1,
                duration_ms = auth_started_at.elapsed().as_secs_f64() * 1000.0,
                "Codex local duration"
            );
        }
        let snap = snapshot.map_err(|_| Error::unauthorized())?;
        if snap.backend_profile != crate::codex::BackendProfile::ChatGptCodex {
            return Err(Error::forbidden());
        }
        let status = a.credentials.status().await;
        if status.generation != snap.generation || !matches!(status.source, "file" | "keyring") {
            return Err(Error::conflict());
        }
        start["credentialStore"] = json!(status.source);
        start["accountId"] = json!(
            snap.auth
                .account_id
                .filter(|id| !id.is_empty())
                .ok_or_else(Error::forbidden)?
        );
        start["codexHome"] = json!(a.credentials.codex_home().ok_or_else(Error::unavailable)?);
        start["codexPath"] =
            json!(binary("PROXY_CODEX_BINARY", "codex").ok_or_else(Error::unavailable)?);
    } else {
        let url = provider_target(a).ok_or_else(Error::unavailable)?;
        endpoint_safe(&url, s).await?;
        start["provider"] = a.provider.clone().ok_or_else(Error::unavailable)?;
        if a.credentials.codex_home().is_some() {
            let capability = access::secret();
            s.turns
                .install_delegate(id, &capability, a.clone(), &c.id)?;
            start["delegateToken"] = capability.into();
            start["delegateURL"] = format!(
                "http://127.0.0.1:{}/api/v1/drivers/codex/delegate",
                s.app.runtime.config.server.port
            )
            .into();
        }
    }
    let mut command = Command::new(binary("PROXY_SDK_BUN", "bun").ok_or_else(Error::unavailable)?);
    command
        .arg(&main)
        .arg(&c.driver)
        .current_dir(&root)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", &root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("LANG", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let starting_since = Instant::now();
    let mut child = command.spawn().map_err(|_| Error::unavailable())?;
    let group = ProcessGroup(child.id());
    let mut stdin = child.stdin.take().ok_or_else(Error::unavailable)?;
    let mut frames = FramedRead::new(
        child.stdout.take().ok_or_else(Error::unavailable)?,
        LinesCodec::new_with_max_length(262144),
    );
    // The embedded OpenCode runtime loads its SDK and plugin graph before readiness.
    // Cold launches can take longer than the lighter Codex worker handshake.
    let (version, ready_limit) = match c.driver.as_str() {
        "opencode" => ("2.3.0", Duration::from_secs(30)),
        _ => ("0.169.0", Duration::from_secs(10)),
    };
    let ready = tokio::select! {
        biased;
        _ = control.recv() => None,
        frame = timeout(ready_limit, frames.next()) => {
            let line = frame.map_err(|_| {
                tracing::warn!(driver = %c.driver, turn_id = id, limit_ms = ready_limit.as_millis() as u64,
                    "Console native worker readiness timed out");
                Error::unavailable()
            })?.and_then(|frame| frame.ok()).ok_or_else(Error::unavailable)?;
            let ready: Value = serde_json::from_str(&line).map_err(|_| Error::unavailable())?;
            if ready["v"] != 1 || ready["type"] != "ready" || ready["driver"] != c.driver
                || ready["version"] != version
            {
                return Err(Error::unavailable());
            }
            Some(Instant::now())
        }
    };
    if ready.is_some() && conversations::binding(a, &c.driver).await.as_deref() != Some(&c.binding)
    {
        return Err(Error::forbidden());
    }
    let ready_at =
        ready.filter(|_| matches!(control.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
    let Some(ready_at) = ready_at else {
        // Only cancellation can arrive before the worker has started a native turn.
        drop(stdin);
        drop(group);
        let _ = child.kill().await;
        conversations::record(
            s,
            &c.id,
            id,
            json!({"type":"terminal","status":"cancelled"}),
        )?;
        return Ok(true);
    };
    let dispatched_at = Instant::now();
    if send(&mut stdin, &start).await.is_err() {
        return Ok(false);
    }
    // After start, a lost worker can have performed an effect. Never replay its input.
    let mut deadline = Instant::now() + Duration::from_secs(900);
    let mut watch = tokio::time::interval(Duration::from_millis(750));
    let mut native_session_ms = None;
    let mut first_text_ms = None;
    let mut terminal_state = "interrupted".to_owned();
    let result = loop {
        tokio::select! {
            _=watch.tick()=>{
                if Instant::now()>deadline||conversations::binding(a,&c.driver).await.as_deref()!=Some(&c.binding){
                    let _=send(&mut stdin,&json!({"v":1,"type":"cancel"})).await;
                    break false;
                }
            }
            cmd=control.recv()=>{
                let Some(v)=cmd else{break false};
                if v["type"]=="cancel"{deadline=deadline.min(Instant::now()+Duration::from_secs(8));}
                if send(&mut stdin,&v).await.is_err(){break false;}
            }
            frame=frames.next()=>{
                let Some(Ok(line))=frame else{break false};
                let Ok(v)=serde_json::from_str::<Value>(&line) else{break false};
                if v["v"]!=1{break false;}
                if v["type"]=="diagnostic_metrics" {
                    if local_metrics { log_codex_metric(id, &v); }
                    continue;
                }
                let done=v["type"]=="terminal";
                match v["type"].as_str() {
                    Some("bound") => { native_session_ms.get_or_insert(dispatched_at.elapsed().as_millis() as u64); }
                    Some("text") => { first_text_ms.get_or_insert(dispatched_at.elapsed().as_millis() as u64); }
                    _ => {}
                }
                let state = done.then(|| {
                    v["status"].as_str().filter(|state| conversations::terminal(state))
                        .unwrap_or("interrupted").to_owned()
                });
                if conversations::record(s,&c.id,id,v).is_err(){break false;}
                if let Some(state) = state {
                    terminal_state = state;
                    break true;
                }
            }
        }
    };
    tracing::info!(
        driver = %c.driver,
        turn_id = id,
        prepare_ms = starting_since.duration_since(preparing_since).as_millis() as u64,
        worker_ready_ms = ready_at.duration_since(starting_since).as_millis() as u64,
        dispatch_ms = dispatched_at.duration_since(preparing_since).as_millis() as u64,
        native_session_ms = ?native_session_ms,
        first_text_ms = ?first_text_ms,
        elapsed_ms = preparing_since.elapsed().as_millis() as u64,
        state = terminal_state,
        "Console native turn timing"
    );
    drop(stdin);
    drop(group);
    let _ = child.kill().await;
    Ok(result)
}
pub(super) async fn run(
    s: Console,
    a: Actor,
    c: Conversation,
    id: String,
    prompt: String,
    rx: mpsc::Receiver<Value>,
) {
    let done = supervise(&s, &a, &c, &id, prompt, rx).await;
    if !matches!(done, Ok(true)) {
        let (status, code) = if done.is_err() {
            ("failed", "pre_dispatch_unavailable")
        } else {
            ("indeterminate", "native_worker_interrupted")
        };
        let _ = conversations::record(
            &s,
            &c.id,
            &id,
            json!({"type":"terminal","status":status,"code":code}),
        );
    }
    s.turns.detach(&id);
}

#[derive(Default)]
pub(super) struct ModelCache {
    owners: tokio::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<ModelSlot>>>,
}
#[derive(Default)]
struct ModelSlot {
    cached: tokio::sync::Mutex<Option<CachedModels>>,
    flight: std::sync::Arc<tokio::sync::Mutex<()>>,
}
struct CachedModels {
    generation: u64,
    epoch: u64,
    recheck_at: Instant,
    usable_until: Instant,
    models: Vec<Value>,
}

impl CachedModels {
    fn usable(&self, a: &Actor) -> bool {
        Instant::now() < self.usable_until
            && a.credentials.codex_home().is_some()
            && a.credentials
                .observed_snapshot(Some(self.epoch))
                .is_ok_and(|snapshot| snapshot.generation == self.generation)
    }
}

pub(super) async fn catalog(s: &Console, a: &Actor) -> Result<Vec<Value>> {
    let slot = s
        .models
        .owners
        .lock()
        .await
        .entry(a.id.clone())
        .or_default()
        .clone();
    let status = a.credentials.status().await;
    let epoch = a.credentials.owner_epoch().await;
    let cached = slot.cached.lock().await;
    let usable = cached
        .as_ref()
        .filter(|entry| status.state == "ready" && entry.epoch == epoch && entry.usable(a));
    if let Some(entry) = usable.filter(|entry| Instant::now() < entry.recheck_at) {
        return Ok(entry.models.clone());
    }
    let stale = usable.map(|entry| entry.models.clone());
    let flight = slot.flight.clone().try_lock_owned();
    drop(cached);

    // Keep the shared load running if an HTTP caller disconnects. While it runs,
    // return the last verified catalogue only to the same credential generation.
    let load = flight.ok().map(|permit| {
        let (slot, s, a) = (slot.clone(), s.clone(), a.clone());
        tokio::spawn(async move {
            let _permit = permit;
            let result = native_catalog(&s, &a, epoch).await;
            let mut cached = slot.cached.lock().await;
            match result {
                Ok(entry) if entry.usable(&a) => {
                    let models = entry.models.clone();
                    *cached = Some(entry);
                    Ok(models)
                }
                Ok(_) => Err(Error::conflict()),
                Err(err) => {
                    if let Some(entry) = cached
                        .as_mut()
                        .filter(|entry| entry.epoch == epoch && entry.usable(&a))
                    {
                        entry.recheck_at = Instant::now() + Duration::from_secs(10);
                    }
                    Err(err)
                }
            }
        })
    });
    if let Some(models) = stale {
        return Ok(models);
    }
    if let Some(load) = load {
        return load.await.map_err(|_| Error::unavailable())?;
    }
    let _permit = slot.flight.lock().await;
    let cached = slot.cached.lock().await;
    cached
        .as_ref()
        .filter(|entry| entry.epoch == epoch && entry.usable(a))
        .map(|entry| entry.models.clone())
        .ok_or_else(Error::unavailable)
}
/// Read the native picker for this credential owner. No session or turn is created.
async fn native_catalog(s: &Console, a: &Actor, epoch: u64) -> Result<CachedModels> {
    use std::collections::HashSet;
    use tokio::process::ChildStdout;

    static ACTIVE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    let _slot = ACTIVE.try_acquire().map_err(|_| Error::busy())?;
    const MAX_LINE: usize = 512 * 1024;
    const MAX_TOTAL: usize = 2 * 1024 * 1024;
    const PAGE: u64 = 32;

    async fn rpc(
        input: &mut ChildStdin,
        output: &mut FramedRead<ChildStdout, LinesCodec>,
        budget: &mut usize,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        send(input, &json!({"id":id,"method":method,"params":params})).await?;
        for _ in 0..32 {
            let line = output
                .next()
                .await
                .and_then(|item| item.ok())
                .ok_or_else(Error::unavailable)?;
            *budget = budget
                .checked_sub(line.len() + 1)
                .ok_or_else(Error::unavailable)?;
            let mut reply: Value = serde_json::from_str(&line).map_err(|_| Error::unavailable())?;
            if reply.get("id").is_none() && reply["method"].is_string() {
                continue;
            }
            if reply["id"].as_u64() != Some(id)
                || reply.get("error").is_some()
                || !reply["result"].is_object()
            {
                return Err(Error::unavailable());
            }
            return Ok(reply["result"].take());
        }
        Err(Error::unavailable())
    }

    let snap = a
        .credentials
        .snapshot_for(Some(epoch))
        .await
        .map_err(|_| Error::unauthorized())?;
    let mut phase = "credential store";
    // A cold App Server can do both a startup refresh and a model/list fetch.
    let result = timeout(Duration::from_secs(30), async {
        if snap.backend_profile != crate::codex::BackendProfile::ChatGptCodex { return Err(Error::forbidden()); }
        let account = snap.auth.account_id.as_deref().filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or_else(Error::forbidden)?;
        let home = a.credentials.codex_home().ok_or_else(Error::unavailable)?
            .canonicalize().map_err(|_| Error::unavailable())?;
        let status = a.credentials.status().await;
        let store = match status.source {
            "file" => "file",
            "keyring" => "keyring",
            _ => return Err(Error::unavailable()),
        };
        if status.generation != snap.generation { return Err(Error::conflict()); }
        let bin = binary("PROXY_CODEX_BINARY", "codex").ok_or_else(Error::unavailable)?;
        phase = "CLI version";
        let cwd = s.root.join("workers").join(access::digest(a.id.as_bytes())).join("catalog");
        access::private_dir(&cwd).map_err(|_| Error::storage())?;
        let mut version_child = Command::new(&bin).arg("--version")
            .current_dir(&cwd).env_clear().env("HOME",&cwd).env("CODEX_HOME",&home).env("PATH","/usr/bin:/bin")
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true)
            .spawn().map_err(|_| Error::unavailable())?;
        let stdout = version_child.stdout.take().ok_or_else(Error::unavailable)?;
        let version = FramedRead::new(stdout, LinesCodec::new_with_max_length(200)).next().await;
        if !matches!(version,Some(Ok(ref line)) if line.trim()=="codex-cli 0.169.0") ||
            !version_child.wait().await.map_err(|_| Error::unavailable())?.success() {
            return Err(Error::unavailable());
        }
        // Pin the native authentication mode and expected workspace. The model/list selector is
        // explicit even when the owner's Codex configuration has a different default provider.
        let mut cmd = Command::new(bin);
        cmd.args(["-c", "model_provider=\"openai\"", "-c", "forced_login_method=\"chatgpt\""])
            .arg("-c").arg(format!("forced_chatgpt_workspace_id={}",toml::Value::String(account.to_owned())))
            .arg("-c").arg(format!("cli_auth_credentials_store=\"{store}\""))
            .args(["-c","analytics.enabled=false","-c","features.plugins=false","-c","features.hooks=false",
                "app-server","--stdio"])
            .current_dir(&cwd).env_clear().env("PATH","/usr/bin:/bin").env("LANG","C")
            .env("HOME",&cwd).env("CODEX_HOME",&home)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
        #[cfg(unix)] cmd.process_group(0);
        let mut child = cmd.spawn().map_err(|_| Error::unavailable())?;
        let _group = ProcessGroup(child.id());
        let mut input = child.stdin.take().ok_or_else(Error::unavailable)?;
        let mut output = FramedRead::new(child.stdout.take().ok_or_else(Error::unavailable)?, LinesCodec::new_with_max_length(MAX_LINE));
        let mut budget = MAX_TOTAL;
        phase = "App Server initialize";
        let initialized = rpc(&mut input,&mut output,&mut budget,1,"initialize",
            json!({"clientInfo":{"name":"openai_proxy","version":env!("CARGO_PKG_VERSION")},"capabilities":null})).await?;
        if initialized["codexHome"].as_str().map(Path::new) != Some(home.as_path()) { return Err(Error::forbidden()); }
        send(&mut input,&json!({"method":"initialized","params":{}})).await?;
        phase = "App Server account/read";
        let identity = rpc(&mut input,&mut output,&mut budget,2,"account/read",json!({"refreshToken":false})).await?;
        if identity["account"]["type"] != "chatgpt" {return Err(Error::forbidden());}

        let mut cursor: Option<String> = None;
        let mut cursors = HashSet::new();
        let mut ids = HashSet::new();
        let mut models = Vec::new();
        phase = "App Server model/list";
        for page in 0..16_u64 {
            let result = rpc(&mut input,&mut output,&mut budget,page+3,"model/list",
                json!({"modelProvider":"openai","cursor":cursor,"limit":PAGE,"includeHidden":false})).await?;
            let entries = result["data"].as_array().ok_or_else(Error::unavailable)?;
            if entries.len() > PAGE as usize { return Err(Error::unavailable()); }
            for entry in entries {
                if entry["hidden"] == true {continue;}
                let id = entry["model"].as_str().filter(|id|valid_model(id)).ok_or_else(Error::unavailable)?;
                if !ids.insert(id.to_owned()) {continue;}
                let mut row = json!({"id":id,"object":"model","owned_by":"openai"});
                if let Some(metadata) = crate::model_catalog::CATALOG.iter().find(|record|record.model_id==id) {
                    row["context_length"] = metadata.context_length_for(snap.backend_profile).into();
                    row["max_output_tokens"] = metadata.max_output_tokens.into();
                }
                models.push(row);
            }
            if result["nextCursor"].is_null() {
                phase = "credential reconciliation";
                let current = a.credentials.reconcile_now().await;
                if current.generation != snap.generation || a.credentials.owner_epoch().await != snap.owner_epoch ||
                    a.credentials.codex_home().is_none() {
                    return Err(Error::conflict());
                }
                let now = Instant::now();
                return Ok(CachedModels {
                    generation: snap.generation,
                    epoch: snap.owner_epoch,
                    recheck_at: now + Duration::from_secs(60),
                    usable_until: now + Duration::from_secs(300),
                    models,
                });
            }
            let next = result["nextCursor"].as_str().filter(|s| !s.is_empty() && s.len()<=4096).ok_or_else(Error::unavailable)?;
            if !cursors.insert(next.to_owned()) {return Err(Error::unavailable());}
            cursor = Some(next.to_owned());
        }
        Err(Error::unavailable())
    }).await;
    result.map_err(|_| {
        tracing::warn!(phase, "Codex model catalogue request timed out");
        Error::unavailable()
    })?
}
