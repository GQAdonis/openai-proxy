"""Five adopted administration rows. Called only by the authorized B4 packet."""
import hashlib
import json
import os
from pathlib import Path
from urllib.parse import urlsplit

from admin_common import MissingPeer, case, http, private, require, snapshot
import admin_setup


SETTINGS = "/api/v1/admin/settings"
ME = "/api/v1/me/connection"
AREAS = set("deployment service identity chat sdk opencode machine acp mcp tools agui a2a skills webhooks memory packaging".split())
TOOLS = {"chat_completion", "list_models", "check_auth", "set_model"}


def inventory(h, actor="local", **options):
    rows = http(h, SETTINGS, actor=actor, **options)["areas"]
    require(len(rows) == len(AREAS) and {v["id"] for v in rows} == AREAS, "Settings inventory differs from the 16 adopted areas")
    for row in rows:
        require(all(row.get(k) for k in ("scope", "source", "activation", "availability")) and
                row["writable"] == bool(row["action"]), "Missing source, scope, timing, state or edit authority")
    return {v["id"]: v for v in rows}


def principal(h, actor):
    manifest = private(h, h.root / "shared/principals.json")
    entries = json.loads(manifest.read_bytes())
    digest = hashlib.sha256(h.keys[actor].encode()).hexdigest()
    return next(p for p in entries if p["tokenSha256"] == digest)


def personal(h, actor):
    mine = principal(h, actor)
    other = principal(h, "chat2" if actor == "chat" else "chat")
    row = http(h, ME, actor=actor)
    body = json.dumps(row)
    require(str(mine["codexHome"]) not in body and str(other["workspace"]) not in body,
            "Personal descriptor exposed a host path")
    provider = mine.get("openCodeProvider") or {}
    different = (other.get("openCodeProvider") or {}).get("model")
    if different and different != provider.get("model"):
        require(different not in row["models"], "Other owner's private model disclosed")
    if provider.get("model"):
        require(provider["model"] in row["models"], "Authenticated user's configured model missing")
    require(row["workspace"] == "default" and row["capabilities"]["reload"] is True and
            isinstance(row["capabilities"]["refresh"], bool), "Invalid connection recovery controls")
    drivers = {d["id"]: d for d in row["drivers"]}
    require(set(drivers) == {"codex","opencode"} and
            all(bool(d["controls"]) == d["available"] for d in drivers.values()), "Unavailable driver advertises controls")
    if not provider:
        require(not drivers["opencode"]["available"], "OpenCode claims availability without a provisioned provider")
    if not different or provider.get("model") == different:
        raise MissingPeer("Two distinct private provider model fixtures are required to discriminate personal visibility")
    return {"role": mine["role"], "privateModelVisible": bool(provider.get("model")), "foreignModelVisible": False}


def descriptors(h, actor):
    origin = h.base if actor == "local" else h.shared
    settings = inventory(h, actor)
    v = {key: row["value"] for key, row in settings.items()}
    me = http(h, ME, actor=actor)
    require(v["deployment"]["mode"] == ("local" if actor == "local" else "shared") and
            v["deployment"]["actorRole"] == "administrator", "Wrong mode or administrator role")
    require(v["service"]["port"]["effective"] == urlsplit(origin).port and
            http(h,"/health",actor=actor), "Advertised service does not match the observed listener")
    require(v["identity"]["generation"] == me["generation"] and v["identity"]["state"] == me["status"],
            "Identity generation and personal state disagree")
    if actor == "local":
        models = http(h, "/v1/models", actor=actor)
        require(isinstance(models["data"], list) and models["data"], "Dedicated owner's direct catalogue unavailable")
    require(
            v["chat"]["modelDiscovery"] == "/v1/models" and v["chat"]["defaultModel"] == me["defaultModel"],
            "Direct catalogue or default model unavailable")
    require(v["sdk"]["drivers"] == me["drivers"] and v["sdk"]["codexSdk"] == "0.169.0"
            and v["sdk"]["openCodeSdk"] == "2.3.0", "SDK descriptor differs from pinned clients or role")
    provider = admin_setup.portable(h, "opencode_provider", actor)
    require(v["opencode"]["provider"] in provider["providers"] and v["opencode"]["certificationTarget"] == "2.3.0",
            "Configured provider or pinned host differs from descriptor")
    require(v["machine"]["scope"] == ["project","global"] and v["machine"]["target"] == "service host",
            "Machine setup scope or destination is misstated")
    descriptor = admin_setup.portable(h, "generic_acp", actor)
    require(descriptor["args"] == v["acp"]["launch"][1:] and not v["acp"]["resume"], "ACP capabilities or launch argv differ")
    require(set(v["mcp"]["tools"]) == TOOLS and
            v["mcp"]["httpPort"] == (int(h.env["MCP_HTTP_PORT"]) if actor == "local" else None),
            "MCP listener/declared tool catalogue mismatch")
    require(not v["tools"]["executor"] and v["tools"]["schemaCount"] == len(v["tools"]["schemas"]),
            "Imported schema count or executor state differs")
    require(v["agui"]["surfaceVersion"] == "1.0" and
            h.http(v["agui"]["structuredEndpoint"],actor=actor)[0] == 405, "Packaged structured route absent")
    enabled = actor == "local"
    card = http(h, "/.well-known/agent.json", actor=actor, statuses=(200,) if enabled else (404,))
    require(v["a2a"]["enabled"]["effective"] is enabled, "A2A enabled flag disagrees with fixture")
    if enabled:
        require(card["url"] == origin and set(v["a2a"]["skills"]) == {s["id"] for s in card["skills"]},
                "A2A advertisement and reachable card differ")
        require(v["skills"]["loaded"] == 2 and v["skills"]["maxInjected"]["effective"] == 1
                and v["tools"]["schemas"] == ["boundary_schema"] and "on_request_received" in v["webhooks"]["events"],
                "Local skills/tool schemas/hooks do not match installed fixture sources")
    else:
        require(v["memory"]["scope"] == "assigned by authenticated principal" and
                "disabled" in v["memory"]["embeddings"], "Shared memory scope or embedding billing differs")
    require(v["memory"]["compiled"] is False and settings["memory"]["activation"] == "build with memory feature",
            "Default release failed to report memory as build-time disabled")
    _, headers, html = h.http("/ui/",actor=actor,headers={"Accept":"text/html"})
    require(b"<html" in html.lower() and "text/html" in headers.get("content-type","")
            and v["packaging"]["memoryCompiled"] is False, "Release packaging inventory differs from installed artifact")
    return {"mode":v["deployment"]["mode"], "areas":sorted(settings), "socketPort":urlsplit(origin).port}


def browser_settings(h, actor):
    from ui_browser import browser
    base = h.base if actor == "local" else h.shared
    with browser(h, base, actor) as page:
        page.get_by_role("link",name="Settings",exact=True).click()
        page.get_by_role("heading",name="My connection",exact=True).wait_for()
        nav=page.get_by_role("navigation",name="Settings areas")
        if actor in ("chat","chat2"):
            require(set(nav.get_by_role("button").all_text_contents())=={"My connection","Client setup"},
                    "Chat browser exposed administrator navigation")
            return {"mode":"chat","adminAreas":0}
        rows=inventory(h,actor)
        for entry in rows.values():
            button=nav.get_by_role("button",name=entry["title"],exact=True)
            button.click()
            page.get_by_role("heading",name=entry["title"],exact=True).wait_for()
            section=page.locator("section").filter(has=page.get_by_role("heading",name=entry["title"],exact=True))
            visible=section.inner_text()
            require(all(v in visible for v in ("Effective value","Source","Scope","Takes effect",entry["source"],entry["activation"])),
                    "Rendered descriptor omitted a value, origin or activation rule")
        return {"mode":actor,"displayedAreas":sorted(rows)}


def conversation_default(h):
    # Both conversations remain private suite data. No turn or worker action is requested.
    original=http(h,ME)
    driver=original["defaultDriver"]
    if not any(d["id"]==driver and d["available"] for d in original["drivers"]):
        raise MissingPeer("The seeded default conversation driver is unavailable")
    path=private(h,Path(h.env["PROXY_CONSOLE_DATA"])/"defaults.json")
    raw=path.read_bytes()
    mode=path.stat().st_mode & 0o777
    before=snapshot(h,path,"conversation defaults before")
    old=http(h,"/api/v1/conversations","POST",{},statuses=(201,))
    try:
        res=http(h,SETTINGS,"PATCH",{"defaultModel":"kbd-new-default"})
        require(res["activation"]=="new conversations" and original["defaultModel"]==old["model"],"Incorrect activation rule")
        new=http(h,"/api/v1/conversations","POST",{},statuses=(201,))
        require(new["model"]=="kbd-new-default" and new["id"]!=old["id"] and
                http(h,"/api/v1/conversations/"+old["id"])["model"]==original["defaultModel"],
                "Existing conversation changed or new conversation did not inherit the edited default")
        require(json.loads(path.read_bytes())["defaultModel"]=="kbd-new-default","Persistence differs from active settings")
    finally:
        http(h,SETTINGS,"PATCH",{"defaultModel":original["defaultModel"]})
        require(http(h,ME)["defaultModel"]==original["defaultModel"],"Could not restore default through API")
        # Restore even the fixture writer's original JSON formatting and mode.
        path.write_bytes(raw)
        os.chmod(path,mode)
        require(snapshot(h,path,"conversation defaults restored")==before,"Exact defaults restoration failed")
    return {"oldModel":old["model"],"newModel":new["model"],"turnsRequested":0,"restored":True}


def config_change(h):
    before = inventory(h)
    field = "skills.max_injected"
    source = before["skills"]["value"]["maxInjected"]
    path = private(h,before["service"]["value"]["config"])
    old = snapshot(h,path,"config before restart-scoped edit")
    preview = http(h, "/api/v1/admin/config/preview", "POST", {"field":field,"value":2})
    require(preview["status"] == "ready" and snapshot(h,path,"after preview") == old, "Preview modified config")
    ident = {"id": preview["id"]}
    try:
        applied = http(h, "/api/v1/admin/config/apply", "POST", ident)
        current = inventory(h)["skills"]["value"]["maxInjected"]
        require(applied["activation"] == "service restart" and current["configured"] == 2
                and current["effective"] == source["effective"] and current["pendingRestart"],
                "Restart-only edit changed the live value or failed to expose pending state")
        require(snapshot(h,path,"persisted config") != old, "Config edit never reached the filesystem")
    finally:
        http(h, "/api/v1/admin/config/undo", "POST", ident)
        require(snapshot(h,path,"after config undo") == old, "Config undo failed to restore source bytes/mode")
    return {"appliedWrites":1,"restored":True,"effective":source["effective"]}


def denied(h, label, method, path, actor, body=None, headers=None):
    before = http(h, ME, actor="admin")
    cfg = inventory(h,"admin")
    config_path = private(h,cfg["service"]["value"]["config"])
    original = snapshot(h,config_path,label + " before")
    status, _, raw = h.http(path,method=method,body=body,actor=actor,headers=headers or {})
    require(status in (400,401,403), f"Protected endpoint returned {status}")
    require(all(token.encode() not in raw for token in h.keys.values()), "A protected response disclosed a synthetic token")
    after = http(h, ME, actor="admin")
    require((before["defaultModel"],before["generation"]) == (after["defaultModel"],after["generation"]) and
            original == snapshot(h,config_path,label + " after"), "Denied operation changed admin state")
    return {"http":status,"configWrites":0,"generationChanges":0}


def admin(h, negative):
    if not negative:
        for actor in ("local","admin"):
            case(h, actor + " settings and consumers", lambda: descriptors(h,actor))
        for actor in ("chat","chat2"):
            case(h, actor + " connection against own private config", lambda: personal(h,actor))
        for actor in ("local","admin","chat","chat2"):
            case(h,actor+" installed settings browser visibility",lambda:browser_settings(h,actor))
        case(h,"new conversations consume changed defaults",lambda:conversation_default(h))
        case(h,"service restart edit persistence and exact undo",lambda: config_change(h))
        return
    for label, method, path, actor, body, headers in [
        ("chat admin inventory","GET",SETTINGS,"chat",None,{}),
        ("chat admin defaults","PATCH",SETTINGS,"chat",{"defaultModel":"kbd-denied"},{}),
        ("invalid browser origin","PATCH",SETTINGS,"admin",{"defaultModel":"kbd-denied"},{"Origin":"https://outside.invalid"}),
        ("untrusted role header","GET",SETTINGS,"chat",None,{"X-Role":"admin"}),
        ("personal subject query","GET",ME+"?subject=admin","chat",None,{}),
        ("foreign refresh subject","POST",ME+"/refresh?subject=admin","chat",{"generation":0},{})]:
        case(h,label,lambda: denied(h,label,method,path,actor,body,headers))


def run(h, row, negative):
    import admin_clients
    import admin_features
    {"ADMIN-1": admin, "SETUP-1":admin_setup.run, "ACP-1":admin_clients.acp,
     "MCP-1":admin_clients.mcp, "FEATURE-1":admin_features.run}[row](h,negative)
