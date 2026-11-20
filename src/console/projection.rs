use super::{
    Console, Error, Result,
    access::Actor,
    conversations::{self, New, Prompt},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Method, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Run {
    thread_id: String,
    run_id: String,
    protocol_version: Option<String>,
    parent_run_id: Option<String>,
    messages: Vec<Value>,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    context: Vec<Value>,
    state: Option<Value>,
    #[serde(default)]
    resume: Vec<Value>,
    #[serde(default)]
    forwarded_props: Value,
}
impl Run {
    fn validate(&self) -> Result<()> {
        // This endpoint projects a durable, server-owned run. History, tool admission,
        // state and native permissions are recovered from that run.
        if Uuid::parse_str(&self.thread_id).is_err()
            || Uuid::parse_str(&self.run_id).is_err()
            || self.protocol_version.as_ref().is_some_and(|v| v != "1.0")
            || self.parent_run_id.is_some()
            || !self.messages.is_empty()
            || !self.tools.is_empty()
            || !self.context.is_empty()
            || !self.resume.is_empty()
            || self
                .state
                .as_ref()
                .is_some_and(|v| !v.is_null() && !v.as_object().is_some_and(|o| o.is_empty()))
            || (!self.forwarded_props.is_null()
                && !self
                    .forwarded_props
                    .as_object()
                    .is_some_and(|o| o.keys().all(|k| k == "a2uiAction")))
        {
            return Err(Error::bad());
        }
        if self.forwarded_props.get("a2uiAction").is_some_and(|v| {
            !v.as_object().is_some_and(|o| {
                o.len() == 2 && o.contains_key("userAction") && o.contains_key("idempotencyKey")
            })
        }) {
            return Err(Error::bad());
        }
        Ok(())
    }
}
fn evt(v: Value) -> std::result::Result<Event, Infallible> {
    Ok(Event::default().data(v.to_string()))
}

pub(super) async fn stream(
    State(s): State<Console>,
    h: HeaderMap,
    Json(run): Json<Run>,
) -> Result<Response> {
    let a = s.actor(&h, Method::POST)?;
    run.validate()?;
    let c = conversations::owned(&s, &a, &run.thread_id).await?;
    if !c.turns.contains_key(&run.run_id) {
        return Err(Error::missing());
    }
    let mut turn = run.run_id.clone();
    if let Some(forward) = run.forwarded_props.get("a2uiAction") {
        let action = forward.get("userAction").ok_or_else(Error::bad)?;
        let key = forward["idempotencyKey"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 120)
            .ok_or_else(Error::bad)?;
        let prompt = format!(
            "The user activated a control in this conversation. Validated action data:\n{}",
            action
        );
        let t = conversations::admit_action(
            &s,
            a.clone(),
            &c.id,
            Prompt {
                prompt,
                idempotency_key: format!("surface/{key}"),
            },
            Some((&run.run_id, action)),
        )
        .await?;
        turn = t.id;
    }
    let items = conversations::stream(s, a, c.id.clone(), turn.clone(), 0).await?;
    let thread = c.id;
    let body = async_stream::stream! {
        yield evt(json!({"type":"RUN_STARTED","threadId":thread,"runId":turn}));
        let message=format!("{turn}-assistant");
        let mut text_open=false;
        let mut finished=false;
        futures::pin_mut!(items);
        while let Some(v)=items.next().await {
            match v["type"].as_str(){
                Some("text")=>{
                    if !text_open{yield evt(json!({"type":"TEXT_MESSAGE_START","messageId":message,"role":"assistant"}));text_open=true;}
                    yield evt(json!({"type":"TEXT_MESSAGE_CONTENT","messageId":message,"delta":v["text"]}));
                }
                Some("surface")=>yield evt(json!({"type":"ACTIVITY_SNAPSHOT","messageId":v["messageId"],"activityType":"a2ui-surface","replace":true,"content":v["content"]})),
                Some("terminal")=>{
                    if text_open{yield evt(json!({"type":"TEXT_MESSAGE_END","messageId":message}));}
                    if v["state"]=="completed"{yield evt(json!({"type":"RUN_FINISHED","threadId":thread,"runId":turn}));}
                    else{yield evt(json!({"type":"RUN_ERROR","message":"The native turn did not complete.","code":v["code"].as_str().unwrap_or("interrupted")}));}
                    finished=true;break;
                }
                _=>{}
            }
        }
        if !finished {yield evt(json!({"type":"RUN_ERROR","message":"Conversation projection interrupted.","code":"interrupted"}));}
    };
    Ok(Sse::new(body)
        .keep_alive(KeepAlive::default())
        .into_response())
}
pub(super) async fn models(State(s): State<Console>, headers: HeaderMap) -> Result<Json<Value>> {
    let actor = s.actor(&headers, Method::GET)?;
    let models = super::worker::catalog(&s, &actor).await?;
    Ok(Json(json!({"object":"list","data":models})))
}
async fn subscribed(
    s: &Console,
    a: Actor,
    model: Option<String>,
    prompt: String,
    key: String,
) -> Result<(String, String, String)> {
    let snap = a
        .credentials
        .snapshot()
        .await
        .map_err(|_| Error::unauthorized())?;
    if snap.backend_profile != crate::codex::BackendProfile::ChatGptCodex {
        return Err(Error::forbidden());
    }
    let c = conversations::new(
        s,
        &a,
        New {
            driver: Some("codex".into()),
            model,
            workspace: None,
        },
    )
    .await?;
    let t = conversations::admit(
        s,
        a,
        &c.id,
        Prompt {
            prompt,
            idempotency_key: key,
        },
    )
    .await?;
    Ok((c.id, t.id, c.model))
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Completion {
    messages: Vec<Message>,
    model: String,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    stream_options: Option<Value>,
}
pub(super) async fn completion(
    State(s): State<Console>,
    h: HeaderMap,
    Json(req): Json<Completion>,
) -> Result<Response> {
    let a = s.actor(&h, Method::POST)?;
    if req.messages.is_empty()
        || req.messages.len() > 128
        || !req.tools.is_empty()
        || req.stream_options.as_ref().is_some_and(|v| {
            v.as_object().is_none_or(|o| {
                o.iter()
                    .any(|(k, v)| k != "include_usage" || !v.is_boolean())
            })
        })
    {
        return Err(Error::bad());
    }
    let mut prompt = String::new();
    for m in req.messages {
        if !matches!(
            m.role.as_str(),
            "system" | "user" | "assistant" | "developer"
        ) {
            return Err(Error::bad());
        }
        prompt.push_str(&m.role);
        prompt.push_str(": ");
        prompt.push_str(&m.content);
        prompt.push('\n');
    }
    let (cid, tid, model) = subscribed(
        &s,
        a.clone(),
        Some(req.model),
        prompt,
        Uuid::new_v4().to_string(),
    )
    .await?;
    let source = conversations::stream(s, a, cid, tid.clone(), 0).await?;
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let include_usage = req
        .stream_options
        .as_ref()
        .and_then(|o| o["include_usage"].as_bool())
        .unwrap_or(false);
    if req.stream {
        let items = async_stream::stream! {
            futures::pin_mut!(source);
            let mut ended=false;let mut usage=None;
            while let Some(v)=source.next().await{
                if v["type"]=="text"{
                    yield evt(json!({"id":tid,"object":"chat.completion.chunk","model":model,"created":created,"choices":[{"index":0,"delta":{"role":"assistant","content":v["text"]},"finish_reason":null}]}));
                }else if v["type"]=="usage"{usage=Some(v["usage"].clone());
                }else if v["type"]=="terminal"{
                    if v["state"]=="completed"{yield evt(json!({"id":tid,"object":"chat.completion.chunk","model":model,"created":created,"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}));}
                    else{yield evt(json!({"error":{"code":subscription_error(&v).1,"message":"The subscription turn did not complete."}}));}
                    if include_usage && v["state"] == "completed" && let Some(counts) = usage.take() {
                        yield evt(json!({"id":tid,"object":"chat.completion.chunk","model":model,"created":created,"choices":[],"usage":counts}));
                    }
                    ended=true;break;
                }
            }
            if !ended{yield evt(json!({"error":{"code":"interrupted","message":"Subscription stream interrupted."}}));}
            yield Ok::<Event,Infallible>(Event::default().data("[DONE]"));
        };
        return Ok(Sse::new(items)
            .keep_alive(KeepAlive::default())
            .into_response());
    }
    let text = collect(source).await?;
    Ok(Json(json!({"id":tid,"model":model,"created":created,"object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}]})).into_response())
}
fn subscription_error(v: &Value) -> Error {
    use axum::http::StatusCode;
    match v["code"].as_str() {
        Some("subscription_quota") => Error(StatusCode::TOO_MANY_REQUESTS, "subscription_quota"),
        Some("subscription_denied") => Error(StatusCode::FORBIDDEN, "subscription_denied"),
        Some("sign_in_required") => Error(StatusCode::UNAUTHORIZED, "sign_in_required"),
        Some("model_unavailable") => Error(StatusCode::NOT_FOUND, "model_unavailable"),
        _ => Error::unavailable(),
    }
}
async fn collect(items: impl futures::Stream<Item = Value>) -> Result<String> {
    futures::pin_mut!(items);
    let mut text = String::new();
    while let Some(v) = items.next().await {
        if v["type"] == "text" {
            text.push_str(v["text"].as_str().unwrap_or_default());
        }
        if v["type"] == "terminal" {
            return if v["state"] == "completed" {
                Ok(text)
            } else {
                Err(subscription_error(&v))
            };
        }
    }
    Err(Error::unavailable())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Delegation {
    parent_session_id: String,
    parent_message_id: String,
    delegation_id: String,
    prompt: String,
}
pub(super) async fn delegate(
    State(s): State<Console>,
    h: HeaderMap,
    Json(req): Json<Delegation>,
) -> Result<Json<Value>> {
    s.access.check_origin(&h, false)?;
    let secret = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(Error::unauthorized)?;
    let (parent_turn, a, cid) = s.turns.delegate_subject(secret)?;
    let c = conversations::owned(&s, &a, &cid).await?;
    if c.driver != "opencode"
        || c.native_id.as_deref() != Some(&req.parent_session_id)
        || c.active.as_deref() != Some(&parent_turn)
        || req.prompt.len() > 65536
        || req.prompt.trim().is_empty()
    {
        return Err(Error::forbidden());
    }
    // IPC and HTTP travel separately. Wait briefly for the exact native permission receipt.
    let mut notify = s.turns.subscribe();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let c = s.turns.read(&cid)?;
        let t = c.turns.get(&parent_turn).ok_or_else(Error::forbidden)?;
        if conversations::terminal(&t.state) {
            return Err(Error::forbidden());
        }
        if t.delegations.get(&req.delegation_id) == Some(&req.parent_message_id) {
            break;
        }
        tokio::time::timeout_at(deadline, notify.changed())
            .await
            .map_err(|_| Error::forbidden())?
            .map_err(|_| Error::forbidden())?;
    }
    s.turns.mutate(&cid, |c| {
        let t = c.turns.get_mut(&parent_turn).ok_or_else(Error::forbidden)?;
        if conversations::terminal(&t.state)
            || t.delegations.remove(&req.delegation_id).as_ref() != Some(&req.parent_message_id)
            || !t.consumed_delegations.insert(req.delegation_id.clone())
        {
            return Err(Error::forbidden());
        }
        Ok(())
    })?;
    let (child, turn, _) = subscribed(
        &s,
        a.clone(),
        None,
        req.prompt,
        format!("delegate/{}", req.delegation_id),
    )
    .await?;
    let items = conversations::stream(s, a, child, turn, 0).await?;
    Ok(Json(json!({"text":collect(items).await?})))
}
