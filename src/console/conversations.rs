use super::{
    Console, Error, Result,
    access::{self, Actor},
    surfaces, worker,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    convert::Infallible,
    fs,
    path::{Path as FsPath, PathBuf},
    sync::Mutex,
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Conversation {
    pub id: String,
    pub driver: String,
    pub model: String,
    pub actor: String,
    pub binding: String,
    pub workspace: PathBuf,
    pub native_id: Option<String>,
    pub active: Option<String>,
    pub turns: BTreeMap<String, Turn>,
    pub surfaces: surfaces::State,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Turn {
    pub id: String,
    pub key: String,
    fingerprint: String,
    pub state: String,
    pub text: String,
    #[serde(default)]
    pub safe_retry: bool,
    pub seq: u64,
    pub events: VecDeque<Value>,
    pub permissions: BTreeMap<String, String>,
    #[serde(default)]
    pub delegations: BTreeMap<String, String>,
    #[serde(default)]
    pub consumed_delegations: std::collections::BTreeSet<String>,
}
pub(super) fn terminal(state: &str) -> bool {
    matches!(
        state,
        "completed" | "cancelled" | "failed" | "indeterminate"
    )
}
impl Turn {
    fn append(&mut self, mut value: Value) {
        self.seq += 1;
        value["sequence"] = self.seq.into();
        self.events.push_back(value);
        while self.events.len() > 256 {
            self.events.pop_front();
        }
    }
    pub fn view(&self) -> Value {
        json!({"id":self.id,"state":self.state,"text":self.text,"sequence":self.seq,"safeRetry":self.safe_retry,"permissions":self.permissions})
    }
}
pub(super) struct Registry {
    dir: PathBuf,
    data: Mutex<BTreeMap<String, Conversation>>,
    delegates: Mutex<BTreeMap<String, (String, Actor, String)>>,
    signals: Mutex<BTreeMap<String, mpsc::Sender<Value>>>,
    notify: watch::Sender<u64>,
}
impl Registry {
    pub fn open(root: &FsPath) -> anyhow::Result<Self> {
        let dir = root.join("conversations");
        access::private_dir(&dir)?;
        let mut data = BTreeMap::new();
        for f in fs::read_dir(&dir)? {
            let f = f?;
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            anyhow::ensure!(
                f.file_type()?.is_file() && f.metadata()?.len() <= 64 * 1024 * 1024,
                "invalid conversation record"
            );
            let mut c: Conversation = serde_json::from_reader(fs::File::open(&path)?)?;
            anyhow::ensure!(
                Uuid::parse_str(&c.id).is_ok()
                    && path.file_stem().and_then(|s| s.to_str()) == Some(&c.id),
                "incorrect record identity"
            );
            if let Some(id) = &c.active
                && let Some(t) = c.turns.get_mut(id).filter(|t| !terminal(&t.state))
            {
                t.state = "indeterminate".into();
                t.permissions.clear();
                t.delegations.clear();
                t.append(json!({"type":"terminal","state":"indeterminate","code":"supervisor_restarted"}));
                access::atomic_json(&path, &c)
                    .map_err(|_| anyhow::anyhow!("could not recover admitted turn"))?;
            }
            data.insert(c.id.clone(), c);
        }
        Ok(Self {
            dir,
            data: Mutex::new(data),
            signals: Mutex::new(BTreeMap::new()),
            delegates: Mutex::new(BTreeMap::new()),
            notify: watch::channel(0).0,
        })
    }
    fn store(&self, c: &Conversation) -> Result<()> {
        access::atomic_json_limited(
            &self.dir.join(format!("{}.json", c.id)),
            c,
            64 * 1024 * 1024,
        )
    }
    pub fn mutate<T>(
        &self,
        id: &str,
        edit: impl FnOnce(&mut Conversation) -> Result<T>,
    ) -> Result<T> {
        let mut map = self.data.lock().map_err(|_| Error::storage())?;
        let mut c = map.get(id).cloned().ok_or_else(Error::missing)?;
        let result = edit(&mut c)?;
        self.store(&c)?;
        map.insert(id.to_owned(), c);
        self.notify.send_modify(|n| *n = n.wrapping_add(1));
        Ok(result)
    }
    pub fn read(&self, id: &str) -> Result<Conversation> {
        self.data
            .lock()
            .map_err(|_| Error::storage())?
            .get(id)
            .cloned()
            .ok_or_else(Error::missing)
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notify.subscribe()
    }
    fn insert(&self, c: Conversation) -> Result<()> {
        let mut map = self.data.lock().map_err(|_| Error::storage())?;
        if map.len() >= 4096 || map.values().filter(|x| x.actor == c.actor).count() >= 1024 {
            return Err(Error::busy());
        }
        self.store(&c)?;
        map.insert(c.id.clone(), c);
        Ok(())
    }
    pub fn command(&self, id: &str, v: Value) -> Result<()> {
        let send = self
            .signals
            .lock()
            .map_err(|_| Error::storage())?
            .get(id)
            .cloned()
            .ok_or_else(Error::conflict)?;
        send.try_send(v).map_err(|_| Error::busy())
    }
    pub fn install_delegate(
        &self,
        turn: &str,
        secret: &str,
        actor: Actor,
        cid: &str,
    ) -> Result<()> {
        self.delegates.lock().map_err(|_| Error::storage())?.insert(
            access::digest(secret.as_bytes()),
            (turn.into(), actor, cid.into()),
        );
        Ok(())
    }
    pub fn delegate_subject(&self, secret: &str) -> Result<(String, Actor, String)> {
        self.delegates
            .lock()
            .map_err(|_| Error::storage())?
            .get(&access::digest(secret.as_bytes()))
            .cloned()
            .ok_or_else(Error::unauthorized)
    }
    pub fn detach(&self, id: &str) {
        if let Ok(mut keys) = self.delegates.lock() {
            keys.retain(|_, (turn, _, _)| turn != id);
        }
        if let Ok(mut signals) = self.signals.lock() {
            signals.remove(id);
        }
    }
}
pub(super) async fn binding(a: &Actor, driver: &str) -> Option<String> {
    let account = a.credentials.account_binding().await;
    if driver == "opencode" {
        return a
            .provider
            .as_ref()
            .map(|p| access::digest(json!([a.id, p, account]).to_string().as_bytes()));
    }
    account
}
pub(super) async fn owned(s: &Console, a: &Actor, id: &str) -> Result<Conversation> {
    let c = s.turns.read(id)?;
    if c.actor != a.id
        || c.workspace != a.workspace
        || binding(a, &c.driver).await.as_deref() != Some(&c.binding)
    {
        return Err(Error::missing());
    }
    Ok(c)
}
pub(super) fn conv_view(c: &Conversation) -> Value {
    json!({"id":c.id,"driver":c.driver,"model":c.model,"workspace":"default","currentTurn":c.active,"capabilities":{"stop":true,"replay":true,"permissions":c.driver=="opencode"}})
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct New {
    pub driver: Option<String>,
    pub model: Option<String>,
    pub workspace: Option<String>,
}
pub(super) async fn new(s: &Console, a: &Actor, req: New) -> Result<Conversation> {
    if req.workspace.as_deref().unwrap_or("default") != "default" {
        return Err(Error::forbidden());
    }
    let prefs = s.settings.get()?;
    let driver = req.driver.unwrap_or(prefs.default_driver);
    if !matches!(driver.as_str(), "codex" | "opencode") {
        return Err(Error::bad());
    }
    let model = req
        .model
        .or(prefs.default_model)
        .or_else(|| s.app.default_model.clone())
        .unwrap_or_else(|| "gpt-5.5".into());
    if !worker::valid_model(&model) {
        return Err(Error::bad());
    }
    if !worker::availability(a, &driver).await.0 {
        return Err(Error::unavailable());
    }
    let binding = binding(a, &driver).await.ok_or_else(Error::unauthorized)?;
    let c = Conversation {
        id: Uuid::new_v4().to_string(),
        actor: a.id.clone(),
        driver,
        model,
        workspace: a.workspace.clone(),
        binding,
        native_id: None,
        active: None,
        turns: BTreeMap::new(),
        surfaces: Default::default(),
    };
    s.turns.insert(c.clone())?;
    Ok(c)
}
pub(super) async fn create(
    State(s): State<Console>,
    h: HeaderMap,
    Json(req): Json<New>,
) -> Result<Response> {
    let a = s.actor(&h, Method::POST)?;
    let c = new(&s, &a, req).await?;
    Ok((StatusCode::CREATED, Json(conv_view(&c))).into_response())
}
pub(super) async fn get(
    State(s): State<Console>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let a = s.actor(&h, Method::GET)?;
    Ok(Json(conv_view(&owned(&s, &a, &id).await?)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Prompt {
    pub prompt: String,
    pub idempotency_key: String,
}
pub(super) async fn admit(s: &Console, a: Actor, cid: &str, req: Prompt) -> Result<Turn> {
    admit_action(s, a, cid, req, None).await
}
pub(super) async fn admit_action(
    s: &Console,
    a: Actor,
    cid: &str,
    req: Prompt,
    action: Option<(&str, &Value)>,
) -> Result<Turn> {
    let c = owned(s, &a, cid).await?;
    if req.prompt.trim().is_empty()
        || req.prompt.len() > 65536
        || req.idempotency_key.is_empty()
        || req.idempotency_key.len() > 160
    {
        return Err(Error::bad());
    }
    let fingerprint = access::digest(req.prompt.as_bytes());
    let (tx, rx) = mpsc::channel(16);
    // Reserve the process slot while writing durable admission. No SDK action precedes this write.
    let admission = {
        let mut slots = s.turns.signals.lock().map_err(|_| Error::storage())?;
        let (t, is_new) = s.turns.mutate(cid, |c| {
            if let Some(t) = c.turns.values().find(|t| t.key == req.idempotency_key) {
                return if t.fingerprint == fingerprint {
                    Ok((t.clone(), false))
                } else {
                    Err(Error::conflict())
                };
            }
            if c.active
                .as_ref()
                .and_then(|id| c.turns.get(id))
                .is_some_and(|t| !terminal(&t.state) || t.state == "indeterminate")
            {
                return Err(Error::conflict());
            }
            if slots.len() >= 16 || c.turns.len() >= 64 {
                return Err(Error::busy());
            }
            if let Some((source, body)) = action {
                if c.active.as_deref() != Some(source) {
                    return Err(Error::conflict());
                }
                c.surfaces.authorize(body, source)?;
            }
            let mut t = Turn {
                id: Uuid::new_v4().to_string(),
                key: req.idempotency_key,
                fingerprint,
                state: "admitted".into(),
                text: String::new(),
                safe_retry: false,
                seq: 0,
                events: VecDeque::new(),
                permissions: BTreeMap::new(),
                delegations: BTreeMap::new(),
                consumed_delegations: Default::default(),
            };
            t.append(json!({"type":"admitted","state":"admitted"}));
            c.active = Some(t.id.clone());
            c.turns.insert(t.id.clone(), t.clone());
            Ok((t, true))
        })?;
        if is_new {
            slots.insert(t.id.clone(), tx);
        }
        (t, is_new)
    };
    let (t, is_new) = admission;
    if is_new {
        let server = s.clone();
        let id = t.id.clone();
        tokio::spawn(async move {
            worker::run(server, a, c, id, req.prompt, rx).await;
        });
    }
    Ok(t)
}
pub(super) async fn start(
    State(s): State<Console>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<Prompt>,
) -> Result<Response> {
    let a = s.actor(&h, Method::POST)?;
    let t = admit(&s, a, &id, req).await?;
    Ok((StatusCode::ACCEPTED, Json(t.view())).into_response())
}
pub(super) async fn turn(
    State(s): State<Console>,
    h: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let a = s.actor(&h, Method::GET)?;
    let c = owned(&s, &a, &id).await?;
    let t = c.turns.get(&tid).ok_or_else(Error::missing)?;
    let mut v = t.view();
    v["surfaces"] = json!(c.surfaces.snapshots(&tid));
    Ok(Json(v))
}
pub(super) fn cursor(h: &HeaderMap) -> Result<u64> {
    h.get("last-event-id")
        .map(|v| {
            v.to_str()
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(Error::bad)
        })
        .transpose()
        .map(|v| v.unwrap_or(0))
}
pub(super) async fn stream(
    s: Console,
    a: Actor,
    cid: String,
    tid: String,
    after: u64,
) -> Result<impl futures::Stream<Item = Value> + Send + 'static> {
    let mut notify = s.turns.subscribe();
    let c = owned(&s, &a, &cid).await?;
    let t = c.turns.get(&tid).ok_or_else(Error::missing)?;
    if after > t.seq
        || t.events
            .front()
            .and_then(|e| e["sequence"].as_u64())
            .is_some_and(|n| after.saturating_add(1) < n)
    {
        return Err(Error(StatusCode::GONE, "replay_cursor_expired"));
    }
    Ok(async_stream::stream! {
        let mut cursor=after;
        loop {
            let Ok(c)=owned(&s,&a,&cid).await else {break};
            let Some(t)=c.turns.get(&tid)else{break};
            if t.events.front().and_then(|e|e["sequence"].as_u64()).is_some_and(|n|cursor.saturating_add(1)<n){yield json!({"type":"resync","state":"expired"});break;}
            for event in &t.events {
                let n=event["sequence"].as_u64().unwrap_or(0);
                if n>cursor {cursor=n;yield event.clone();}
            }
            if terminal(&t.state){break;}
            tokio::select! {res=notify.changed()=>{if res.is_err(){break;}},_=tokio::time::sleep(std::time::Duration::from_secs(5))=>{}}
        }
    })
}
pub(super) async fn events(
    State(s): State<Console>,
    h: HeaderMap,
    Path((id, t)): Path<(String, String)>,
) -> Result<Response> {
    use futures::StreamExt;
    let a = s.actor(&h, Method::GET)?;
    let recovery = format!("/api/v1/conversations/{id}/turns/{t}");
    let items = match stream(s, a, id, t, cursor(&h)?).await {
        Ok(items) => items,
        Err(Error(StatusCode::GONE, code)) => {
            return Ok((
                StatusCode::GONE,
                Json(json!({"error":{"code":code},"state":recovery})),
            )
                .into_response());
        }
        Err(e) => return Err(e),
    };
    Ok(Sse::new(items.map(|v| {
        Ok::<_, Infallible>(
            Event::default()
                .id(v["sequence"].to_string())
                .data(v.to_string()),
        )
    }))
    .keep_alive(KeepAlive::default())
    .into_response())
}
pub(super) async fn cancel(
    State(s): State<Console>,
    h: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let a = s.actor(&h, Method::POST)?;
    owned(&s, &a, &id).await?;
    let (v, send) = s.turns.mutate(&id, |c| {
        let t = c.turns.get_mut(&tid).ok_or_else(Error::missing)?;
        let send = !terminal(&t.state);
        if send && t.state != "cancelling" {
            t.state = "cancelling".into();
            t.append(json!({"type":"status","state":"cancelling"}));
        }
        Ok((t.view(), send))
    })?;
    if send {
        s.turns.command(&tid, json!({"v":1,"type":"cancel"}))?;
    }
    Ok(Json(v))
}
pub(super) async fn permission(
    State(s): State<Console>,
    h: HeaderMap,
    Path((id, tid, request)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let a = s.actor(&h, Method::POST)?;
    owned(&s, &a, &id).await?;
    let decision = body["decision"]
        .as_str()
        .filter(|d| matches!(*d, "allow" | "deny"))
        .ok_or_else(Error::bad)?;
    s.turns.mutate(&id, |c| {
        let t = c.turns.get_mut(&tid).ok_or_else(Error::missing)?;
        if t.permissions.remove(&request).is_none() {
            return Err(Error::conflict());
        }
        Ok(())
    })?;
    s.turns.command(
        &tid,
        json!({"v":1,"type":"permission","requestId":request,"decision":decision}),
    )?;
    Ok(Json(json!({"status":"submitted"})))
}
pub(super) fn record(s: &Console, cid: &str, tid: &str, v: Value) -> Result<()> {
    s.turns.mutate(cid,|c|{
        let t=c.turns.get_mut(tid).ok_or_else(Error::missing)?;
        if terminal(&t.state){return Err(Error::conflict());}
        match v["type"].as_str() {
            Some("bound")=>{
                let native=v["nativeId"].as_str().filter(|x|!x.is_empty()&&x.len()<=512).ok_or_else(Error::bad)?;
                if c.native_id.as_deref().is_some_and(|old|old!=native){return Err(Error::conflict());}
                c.native_id=Some(native.to_owned());if t.state!="cancelling"{t.state="running".into();} t.append(json!({"type":"status","state":t.state}));
            }
            Some("text")=>{
                let text=v["text"].as_str().ok_or_else(Error::bad)?;
                if t.text.len()+text.len()>512*1024{return Err(Error::busy());}
                t.text.push_str(text);t.append(json!({"type":"text","text":text}));
            }
            Some("usage")=>{
                let mut counts=serde_json::Map::new();
                for key in ["prompt_tokens","completion_tokens","total_tokens"]{
                    counts.insert(key.into(),json!(v[key].as_u64().ok_or_else(Error::bad)?));
                }
                t.append(json!({"type":"usage","usage":counts}));
            }
            Some("permission")=>{
                let id=v["requestId"].as_str().filter(|x|!x.is_empty()&&x.len()<=256).ok_or_else(Error::bad)?;
                let label=v["label"].as_str().filter(|x|x.len()<=1024).unwrap_or("Native permission request");
                if t.permissions.len()>=32{return Err(Error::busy());}t.permissions.insert(id.to_owned(),label.to_owned());
                t.append(json!({"type":"permission","requestId":id,"label":label}));
            }
            Some("tool")=>{
                let status=v["status"].as_str().ok_or_else(Error::bad)?;
                if status=="delegation_authorized" && c.driver=="opencode" {
                    let parent=v["parentMessageId"].as_str().filter(|x|!x.is_empty()&&x.len()<=256).ok_or_else(Error::bad)?;
                    let key=v["delegationId"].as_str().filter(|x|!x.is_empty()&&x.len()<=256).ok_or_else(Error::bad)?;
                    if t.delegations.len()>=32{return Err(Error::busy());}
                    if !t.consumed_delegations.contains(key){t.delegations.entry(key.to_owned()).or_insert_with(||parent.to_owned());}
                }
                t.append(json!({"type":"tool","status":status}));
            }
            Some("surface")=>{match c.surfaces.accept(&v,tid) {
                    Ok(event)=>t.append(event),
                    Err(_)=>t.append(json!({"type":"surface_error","reason":"The interactive content did not pass validation."})),
                }}
            Some("terminal")=>{
                let state=v["status"].as_str().filter(|x|terminal(x)).ok_or_else(Error::bad)?;
                t.safe_retry=state=="failed"&&v["code"]=="pre_dispatch_unavailable";t.state=state.into();t.permissions.clear();t.delegations.clear();
                t.append(json!({"type":"terminal","state":state,"safeRetry":t.safe_retry,"code":v.get("code").and_then(Value::as_str).filter(|s|s.len()<80)}));
            }
            _=>return Err(Error::bad())
        }Ok(())
    })
}
