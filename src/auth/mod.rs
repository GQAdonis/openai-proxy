//! Live credentials for one explicitly selected store owner.
mod binding;
mod owner;
mod store;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use notify::Watcher;
use serde::Serialize;
use tokio::sync::{Mutex as AsyncMutex, Notify};

use crate::{
    codex::{BackendProfile, CodexAuth},
    error::ProxyError,
};
use store::{Mode, ReadError, Record, Store};

#[derive(Clone)]
pub struct CredentialProvider(Arc<Inner>);

struct Inner {
    source: Store,
    state: Mutex<State>,
    scan: AsyncMutex<()>,
    refresh: AsyncMutex<()>,
    changed: Notify,
    native_runtime: owner::Runtime,
}

struct State {
    record: Option<Record>,
    observed: &'static str,
    source_hint: &'static str,
    generation: u64,
    owner_epoch: u64,
    reconciled: u64,
    checked: Instant,
    binding: Option<String>,
    renewed: Option<u64>,
    pending_confirmation: Option<u64>,
    blocked: Option<&'static str>,
    retry_after: Option<Instant>,
    billing_class: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConnectionStatus {
    pub generation: u64,
    pub state: &'static str,
    pub mode: &'static str,
    pub source: &'static str,
    pub can_refresh: bool,
    pub refresh_runtime: &'static str,
    pub last_reconciled: u64,
    pub last_renewed: Option<u64>,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct CredentialSnapshot {
    pub generation: u64,
    pub owner_epoch: u64,
    pub backend_profile: BackendProfile,
    pub(crate) backend_url: String,
    pub(crate) auth: CodexAuth,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn same_owner(a: &Record, b: &Record) -> bool {
    if a.mode == Mode::ApiKey || b.mode == Mode::ApiKey {
        a.mode == b.mode && a.auth.api_key == b.auth.api_key
    } else {
        a.auth.account_id == b.auth.account_id
    }
}

pub(super) fn topology_ok(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if std::fs::read_to_string("/proc/self/mountinfo")
            .ok()
            .is_some_and(|text| {
                text.lines()
                    .filter_map(|line| line.split_whitespace().nth(4))
                    .any(|mount| Path::new(&mount.replace("\\040", " ")) == canonical)
            })
        {
            return false;
        }
    }
    let _ = path;
    true
}

impl CredentialProvider {
    pub fn disconnected() -> Self {
        let mut source = Store::file(PathBuf::from("disabled-owner"));
        source.environment_key = Some(String::new());
        Self::new(source)
    }

    pub fn process(
        path: Option<PathBuf>,
        wire_api: &str,
        url: Option<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self::new(Store::process(path, wire_api, url)?))
    }

    pub fn for_home(home: PathBuf) -> anyhow::Result<Self> {
        anyhow::ensure!(home.is_absolute(), "credential home must be absolute");
        Ok(Self::new(Store::file(home.join("auth.json"))))
    }

    fn new(source: Store) -> Self {
        let provider = Self(Arc::new(Inner {
            source,
            scan: AsyncMutex::new(()),
            refresh: AsyncMutex::new(()),
            changed: Notify::new(),
            native_runtime: Default::default(),
            state: Mutex::new(State {
                record: None,
                observed: "signed_out",
                source_hint: "none",
                generation: 0,
                owner_epoch: 0,
                binding: None,
                reconciled: 0,
                checked: Instant::now(),
                renewed: None,
                pending_confirmation: None,
                blocked: None,
                retry_after: None,
                billing_class: None,
            }),
        }));
        provider.publish(provider.0.source.read());
        if provider.0.source.environment_key.is_none()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let weak = Arc::downgrade(&provider.0);
            let path = provider.0.source.path.clone();
            runtime.spawn(async move {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);
                let parent = path.parent().unwrap_or(Path::new(".")).to_owned();
                let mut watcher = None;
                let mut poll = tokio::time::interval(Duration::from_secs(20));
                loop {
                    tokio::select! {
                        _ = poll.tick() => {
                            watcher.take();
                            let hint = tx.clone();
                            let watched = path.clone();
                            let config = parent.join("config.toml");
                            if let Ok(mut w) = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                                if event.map_or(true, |e| !matches!(e.kind, notify::EventKind::Access(_))
                                    && e.paths.iter().any(|p| p == &watched || p == &config || watched.parent() == Some(p.as_path()))) {
                                    let _ = hint.try_send(());
                                }
                            }) && w.watch(&parent, notify::RecursiveMode::NonRecursive).is_ok() { watcher = Some(w) }
                        }
                        _ = rx.recv() => tokio::time::sleep(Duration::from_millis(60)).await,
                    }
                    let Some(inner) = weak.upgrade() else { break };
                    Self(inner).reconcile().await;
                }
            });
        }
        provider
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn native_path(&self) -> Option<&Path> {
        self.0
            .source
            .native
            .then(|| self.0.source.home())
            .filter(|_| topology_ok(&self.0.source.path) && !self.0.source.path.is_symlink())
    }

    pub(crate) fn credential_path(&self) -> &Path {
        &self.0.source.path
    }

    pub fn codex_home(&self) -> Option<&Path> {
        let state = self.state();
        if state.observed != "ready"
            || !state
                .record
                .as_ref()
                .is_some_and(|r| r.mode == Mode::Managed)
            || matches!(state.blocked, Some("sign_in_required" | "token_rejected"))
        {
            return None;
        }
        self.native_path()
    }

    fn can_write_store(&self) -> bool {
        self.native_path()
            .and_then(|path| std::fs::metadata(path).ok())
            .is_some_and(|meta| meta.is_dir() && !meta.permissions().readonly())
    }

    fn publish(&self, result: Result<Record, ReadError>) {
        let mut state = self.state();
        let first_read = state.reconciled == 0;
        state.checked = Instant::now();
        state.reconciled = now();
        let next = match result {
            Ok(record) => {
                state.source_hint = record.source;
                let billed = record.mode == Mode::ApiKey;
                if state.billing_class.is_some_and(|prior| prior != billed)
                    || self.0.source.route(&record).is_none()
                {
                    state.observed = "routing_authorization_required";
                    None
                } else {
                    state.billing_class = Some(billed);
                    state.observed = "ready";
                    Some(record)
                }
            }
            Err(ReadError::Missing) => {
                state.observed = "signed_out";
                None
            }
            Err(ReadError::Unsupported(source)) => {
                state.source_hint = source;
                state.observed = "unsupported_store";
                None
            }
            Err(ReadError::Partial) => {
                state.observed = "incomplete_store";
                return;
            }
            Err(ReadError::Unavailable) => {
                state.observed = "store_unavailable";
                return;
            }
        };
        let owner_changed =
            !matches!((&state.record, &next), (Some(old), Some(new)) if same_owner(old,new));
        if ((first_read || state.record != next) && owner_changed)
            || (next.is_some() && state.binding.is_none())
        {
            let owner = next
                .as_ref()
                .and_then(|r| r.auth.account_id.as_deref().or(r.auth.api_key.as_deref()));
            if self.0.source.native || self.0.source.environment_key.as_deref() != Some("") {
                state.binding = binding::update(&self.0.source.path, owner).ok().flatten();
            }
        }
        if state.record != next {
            if owner_changed {
                state.owner_epoch += 1;
                state.renewed = None;
            }
            state.generation += 1;
            state.pending_confirmation = None;
            state.blocked = None;
            state.retry_after = None;
            state.record = next;
            self.0.changed.notify_waiters();
        }
    }

    async fn reconcile(&self) {
        let _scan = self.0.scan.lock().await;
        let source = self.0.source.clone();
        let read = tokio::task::spawn_blocking(move || source.read())
            .await
            .unwrap_or(Err(ReadError::Unavailable));
        self.publish(read);
    }

    async fn recent(&self) {
        if self.state().checked.elapsed() >= Duration::from_millis(900) {
            self.reconcile().await
        }
    }

    pub async fn owner_epoch(&self) -> u64 {
        self.recent().await;
        self.state().owner_epoch
    }

    pub async fn account_binding(&self) -> Option<String> {
        self.recent().await;
        let state = self.state();
        if state.observed != "ready"
            || matches!(state.blocked, Some("sign_in_required" | "token_rejected"))
        {
            return None;
        }
        state.binding.clone()
    }

    /// Check the file-identified native CLI against the supported App Server release.
    /// This isolated version query never opens the selected credential directory.
    pub async fn native_cli_compatible(&self) -> bool {
        self.0.native_runtime.compatibility().await == owner::Compatibility::Available
    }

    pub async fn observed_profile(&self) -> Option<BackendProfile> {
        self.recent().await;
        let state = self.state();
        if state.observed != "ready"
            || matches!(state.blocked, Some("sign_in_required" | "token_rejected"))
        {
            return None;
        }
        let record = state.record.as_ref()?;
        self.0.source.route(record).map(|(_, profile)| profile)
    }

    pub async fn reconcile_now(&self) -> ConnectionStatus {
        self.reconcile().await;
        self.description().await
    }

    pub async fn status(&self) -> ConnectionStatus {
        self.recent().await;
        self.description().await
    }

    async fn description(&self) -> ConnectionStatus {
        let eligible = {
            let state = self.state();
            state.observed == "ready"
                && state
                    .record
                    .as_ref()
                    .is_some_and(|r| r.mode == Mode::Managed)
        };
        let capability =
            if eligible && self.can_write_store() && owner::broker_available(&self.0.source) {
                Some(self.0.native_runtime.compatibility().await)
            } else {
                None
            };
        let state = self.state();
        let record = state.record.as_ref();
        let expired = record
            .and_then(|r| r.expiry)
            .is_some_and(|expiry| expiry <= now());
        let status = if state.observed != "ready" {
            state.observed
        } else {
            state
                .blocked
                .unwrap_or(if expired { "expired" } else { "ready" })
        };
        ConnectionStatus {
            generation: state.generation,
            state: status,
            mode: record.map_or("none", |r| r.mode.name()),
            source: state.source_hint,
            can_refresh: state.observed == "ready"
                && state.blocked.is_none()
                && state
                    .retry_after
                    .is_none_or(|deadline| deadline <= Instant::now())
                && record.is_some_and(|r| r.mode == Mode::Managed)
                && self.can_write_store()
                && capability == Some(owner::Compatibility::Available),
            refresh_runtime: if eligible && !owner::broker_available(&self.0.source) {
                "keyring_broker_unavailable"
            } else {
                capability.map_or("not_applicable", |v| v.name())
            },
            last_reconciled: state.reconciled,
            last_renewed: state.renewed,
            retry_after_seconds: state
                .retry_after
                .map(|t| t.saturating_duration_since(Instant::now()).as_secs()),
        }
    }

    pub async fn refresh(&self, expected_generation: u64) -> ConnectionStatus {
        // Keep the single-flight task alive if the requesting HTTP connection disconnects.
        let provider = self.clone();
        let _ = tokio::spawn(async move { provider.rotate(expected_generation).await }).await;
        self.description().await
    }

    async fn rotate(&self, generation: u64) {
        let _exclusive = self.0.refresh.lock().await;
        self.reconcile().await;
        let (record, epoch) = {
            let s = self.state();
            let Some(record) = s.record.clone() else {
                return;
            };
            if s.generation != generation
                || s.observed != "ready"
                || s.blocked.is_some()
                || s.retry_after.is_some_and(|t| t > Instant::now())
                || record.mode != Mode::Managed
                || !self.can_write_store()
            {
                return;
            }
            (record, s.owner_epoch)
        };
        let outcome = owner::exchange(&self.0.native_runtime, &self.0.source, &record, || {
            let state = self.state();
            state.generation == generation
                && state.owner_epoch == epoch
                && state.observed == "ready"
        });
        tokio::pin!(outcome);
        let mut audit = tokio::time::interval(Duration::from_millis(250));
        let result = loop {
            tokio::select! {
                result = &mut outcome => break result,
                _ = self.0.changed.notified() => if self.state().owner_epoch != epoch { return },
                _ = audit.tick() => if self.state().owner_epoch != epoch { return },
            }
        };
        self.reconcile().await;
        let mut state = self.state();
        if state.owner_epoch != epoch || state.observed != "ready" {
            return;
        }
        if state.generation != generation {
            if matches!(result, owner::Outcome::Reconcile)
                && let Some(new) = &state.record
                && new.auth.access_token != record.auth.access_token
            {
                if new.expiry.is_some_and(|e| e > now()) {
                    state.renewed = Some(now());
                } else if new.expiry.is_none() {
                    state.pending_confirmation = Some(state.generation);
                }
            }
            return;
        }
        match result {
            owner::Outcome::SignInRequired => state.blocked = Some("sign_in_required"),
            owner::Outcome::Indeterminate | owner::Outcome::Reconcile => {
                state.blocked = Some("refresh_unconfirmed")
            }
            owner::Outcome::RetryLater => {
                state.retry_after = Some(Instant::now() + Duration::from_secs(30))
            }
            owner::Outcome::Superseded => {}
        }
    }

    pub async fn is_current(&self, generation: u64) -> bool {
        self.recent().await;
        let s = self.state();
        s.generation == generation
            && s.record.is_some()
            && s.observed == "ready"
            && s.blocked != Some("sign_in_required")
            && s.blocked != Some("token_rejected")
    }

    pub(crate) async fn rejected(&self, generation: u64) {
        self.refresh(generation).await;
        let mut s = self.state();
        if s.generation == generation && s.blocked != Some("sign_in_required") {
            s.blocked = Some("token_rejected");
        }
    }

    pub(crate) fn authenticated(&self, generation: u64) {
        let mut s = self.state();
        if s.generation != generation {
            return;
        }
        if s.pending_confirmation == Some(generation) {
            s.pending_confirmation = None;
            s.renewed = Some(now());
        }
        if s.blocked == Some("refresh_unconfirmed") {
            s.blocked = Some("reused");
        }
    }

    pub async fn snapshot(&self) -> Result<CredentialSnapshot, ProxyError> {
        self.snapshot_for(None).await
    }

    pub(crate) async fn snapshot_for(
        &self,
        expected_epoch: Option<u64>,
    ) -> Result<CredentialSnapshot, ProxyError> {
        self.recent().await;
        let (epoch, proactive) = {
            let state = self.state();
            if expected_epoch.is_some_and(|expected| expected != state.owner_epoch) {
                return Err(ProxyError::AuthUnavailable(
                    "request login was superseded".into(),
                ));
            }
            let refresh = state
                .record
                .as_ref()
                .filter(|r| {
                    r.mode == Mode::Managed
                        && r.expiry.is_some_and(|exp| exp <= now().saturating_add(60))
                })
                .map(|_| state.generation);
            (state.owner_epoch, refresh)
        };
        if let Some(generation) = proactive {
            self.refresh(generation).await;
        }
        self.observed_snapshot(Some(epoch))
    }

    pub(crate) fn observed_snapshot(
        &self,
        expected_epoch: Option<u64>,
    ) -> Result<CredentialSnapshot, ProxyError> {
        let state = self.state();
        let error = || {
            ProxyError::AuthUnavailable(
                "owner sign-in or credential reconciliation required".into(),
            )
        };
        if expected_epoch.is_some_and(|epoch| state.owner_epoch != epoch)
            || state.observed != "ready"
            || matches!(state.blocked, Some("sign_in_required" | "token_rejected"))
        {
            return Err(error());
        }
        let record = state.record.as_ref().ok_or_else(error)?;
        if record.expiry.is_some_and(|e| e <= now()) {
            return Err(error());
        }
        let (backend_url, backend_profile) = self.0.source.route(record).ok_or_else(error)?;
        Ok(CredentialSnapshot {
            generation: state.generation,
            owner_epoch: state.owner_epoch,
            auth: record.auth.clone(),
            backend_url,
            backend_profile,
        })
    }
}

pub fn setup_profile() -> anyhow::Result<BackendProfile> {
    let store = Store::process(
        None,
        &std::env::var("CODEX_WIRE_API").unwrap_or_default(),
        std::env::var("CODEX_BACKEND_URL").ok(),
    )?;
    let record = store
        .read()
        .map_err(|_| anyhow::anyhow!("no available credential for the selected owner"))?;
    store
        .route(&record)
        .map(|(_, profile)| profile)
        .ok_or_else(|| anyhow::anyhow!("credential class incompatible with configured upstream"))
}

pub(crate) fn read_selected(path: &Path) -> anyhow::Result<CodexAuth> {
    Store::file(path.to_owned())
        .read()
        .map(|record| record.auth)
        .map_err(|_| {
            anyhow::anyhow!(
                "selected credential missing or unreadable (file or configured Codex Auth keyring)"
            )
        })
}
