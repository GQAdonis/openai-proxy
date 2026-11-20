"""Installed SDK drivers, the installed V2 plugin and observable native effects."""
import json
import sys
from contextlib import nullcontext
from auth_file_observer import watch_reads
import secrets
from pathlib import Path

from auth_fixture import credential, fixture, wait_for
from identity import install, status_json


def marker(case):
    return "kbd-" + case + "-" + secrets.token_hex(6)


def model_requests(h, key):
    p = h.root / "native-model-observer.jsonl"
    return [row for line in p.read_text().splitlines() if (row := json.loads(line))["marker"] == key] if p.exists() else []


def disconnected(h, key):
    p = h.root / "upstream-observations.jsonl"
    return any(key in event.get("markers", []) and event.get("disconnected") for line in p.read_text().splitlines()
               if (event := json.loads(line))) if p.exists() else False


def installed(h, driver):
    base = h.root / "install/lib/openai-proxy/workers"
    dep = "@openai/codex-sdk" if driver == "codex" else "@opencode/sdk"
    return (base / "main.mjs").is_file() and (base / "node_modules" / dep / "package.json").is_file() and bool(h.env.get("PROXY_SDK_BUN"))


def conversation(f, driver):
    return status_json(f.api("/api/v1/conversations", "POST", {"driver": driver,
                       "model": "gpt-5.5" if driver == "codex" else "fixture-chat"}), 201)["id"]


def start(f, cid, text, key=None):
    item = status_json(f.api(f"/api/v1/conversations/{cid}/turns", "POST",
                       {"prompt": text, "idempotencyKey": key or secrets.token_hex(10)}), 202)
    return f"/api/v1/conversations/{cid}/turns/" + item["id"]


def view(f, path):
    return status_json(f.api(path), 200)


def finish(f, path, wanted="completed"):
    def done():
        turn = view(f, path)
        if turn["state"] in {"completed", "failed", "indeterminate", "cancelled"}:
            assert turn["state"] == wanted, "unexpected terminal state " + turn["state"]
            return turn
    return wait_for(done, 35)


def events(f, path, after=0):
    code, _, raw = f.api(path + "/events", headers={"Last-Event-ID": str(after)})
    assert code == 200, f"event stream HTTP {code}"
    data = [json.loads(line[5:]) for line in raw.splitlines() if line.startswith(b"data:")]
    positions = [e["sequence"] for e in data]
    assert positions == list(range(after + 1, after + len(data) + 1)), "event sequence gap or replay duplicate"
    return data


def durable_id(f, cid):
    record = json.loads((f.root / "console/conversations" / (cid + ".json")).read_text())
    assert record["native_id"] and Path(record["workspace"]).resolve() == f.workspace.resolve()
    return record["native_id"]


def chat(h, driver, negative):
    with fixture(h, workers=True) as f:
        install(f, credential("owner-a", managed=True))
        cid = conversation(f, driver)
        key = marker("chat-answer")
        path = start(f, cid, key, key)
        turn = finish(f, path)
        transcript = events(f, path)
        assert sum(e["type"] == "terminal" for e in transcript) == 1
        assert "".join(e["text"] for e in transcript if e["type"] == "text") == turn["text"]
        reply = "fixture reply" if driver == "codex" else "reply-" + key
        assert turn["text"] == reply
        assert events(f, path, 1) == transcript[1:]
        if negative:
            assert start(f, cid, key, key) == path
            assert f.api(f"/api/v1/conversations/{cid}/turns", "POST",
                         {"idempotencyKey": key, "prompt": "different input"})[0] == 409
            assert f.api(path + "/events", headers={"Last-Event-ID": str(turn["sequence"] + 1)})[0] == 410
            assert f.api("/api/v1/conversations", "POST", {"driver": driver, "workspace": "other"})[0] == 403
            assert f.api(f"/api/v1/conversations/{cid}/turns", "POST",
                         {"driver": "other", "prompt": key, "idempotencyKey": "wrong-driver"})[0] in (400, 422)
            for _ in range(2):
                assert status_json(f.api(path + "/cancel", "POST", {}), 200)["state"] == "completed"
            assert transcript == events(f, path)
            if driver == "codex":
                assert len(f.events("exec")) == 1
            else:
                assert len(model_requests(h, key)) == 1
            return {"driver": driver, "admissions": 1, "already_completed_cancel_effects": 0}
        native = durable_id(f, cid)
        if driver == "codex":
            assert f.events("exec")[0]["native"] == native
        else:
            assert len(model_requests(h, key)) == 1
        f.stop_service()
        f.start_service()
        assert events(f, path) == transcript and durable_id(f, cid) == native
        key = marker("chat-hold")
        held = start(f, cid, key + (" KBD_HOLD" if driver == "codex" else ""))
        if driver == "codex":
            calls = wait_for(lambda: f.events("exec") if len(f.events("exec")) >= 2 else None)
            assert calls[-1]["resume"] and calls[-1]["native"] == native
        else:
            wait_for(lambda: model_requests(h, key))
        assert f.api(held + "/cancel", "POST", {})[0] == 200
        finish(f, held, "cancelled")
        if driver == "codex":
            wait_for(lambda: f.events("interrupt"))
        else:
            wait_for(lambda: disconnected(h, key), 7)
        assert durable_id(f, cid) == native
        assert sum(e["type"] == "terminal" for e in events(f, held)) == 1
        return {"driver": driver, "service_restarted": True, "same_native_id": True,
                "external_cancel_observed": "CLI peer SIGTERM" if driver == "codex" else "model socket disconnected"}


def shared(h):
    path = "/api/v1/conversations"
    created = h.http(path, method="POST", body={"driver": "opencode", "model": "fixture-chat"}, base=h.shared, actor="admin")
    if created[0] == 503:
        raise FileNotFoundError("shared admin has no configured OpenCode worker")
    cid = status_json(created, 201)["id"]
    assert h.http(path + "/" + cid, base=h.shared, actor="admin")[0] == 200
    code, _, raw = h.http(path + "/" + cid, base=h.shared, actor="chat")
    assert code in (403, 404) and cid.encode() not in raw
    return {"foreign_session_http": code}


def tool(h, negative, delegate=False):
    with fixture(h, workers=True) as f:
        install(f, credential("owner-a", managed=True))
        key = marker("tool-delegate" if delegate else "tool-read")
        if not delegate:
            (f.workspace / (key + ".txt")).write_text("value-" + key)
        cid = conversation(f, "opencode")
        watcher = nullcontext(None) if delegate else watch_reads(f.workspace / (key + ".txt"))
        with watcher as observed:
            path = start(f, cid, key)
            def pending():
                state = view(f, path)
                assert state["state"] not in ("failed", "indeterminate", "completed", "cancelled"), "native request ended before permission"
                return state["permissions"]
            permissions = wait_for(pending, 30)
            assert len(permissions) == 1
            ident, label = next(iter(permissions.items()))
            assert label == ("delegate_codex" if delegate else "workspace_read")
            endpoint = path + "/permissions/" + ident
            assert not any(r["canary_returns"] or r["delegated_returns"] for r in model_requests(h, key))
            if negative:
                assert f.api(path + "/permissions/expired-fixture", "POST", {"decision": "allow"})[0] == 409
            assert f.api(endpoint, "POST", {"decision": "deny" if negative else "allow"})[0] == 200
            terminal = "cancelled" if negative else "completed"
            finish(f, path, terminal)
            transcript = events(f, path)
            assert [e["state"] for e in transcript if e["type"] == "terminal"] == [terminal], "unexpected terminal event history"
            assert events(f, path) == transcript
            assert f.api(endpoint, "POST", {"decision": "allow"})[0] == 409
            reports = model_requests(h, key)
            assert reports, "independent model recipient did not observe the marked turn"
            count = sum(r["delegated_returns" if delegate else "canary_returns"] for r in reports)
            assert count == (0 if negative else 1), "independent recipient result count differs"
            assert sum(e["type"] == "permission" and e["requestId"] == ident for e in transcript) == 1
            if delegate:
                assert len(f.events("exec")) == count
            if observed is not None:
                assert observed() == [count, count], "canary file-open count differs from authorized effect count"
            return {"tool": label, "authorized": not negative, "terminal": terminal,
                    "observed_tool_results": count, "permission_requests": 1}



def unsupported(h):
    with fixture(h, workers=True) as f:
        install(f, credential("owner-a", managed=True))
        req = {"model": "gpt-5.5", "messages": [{"role": "user", "content": "fixture"}],
               "tools": [{"type": "function", "function": {"name": "unavailable", "parameters": {"type": "object"}}}]}
        registry = Path(f.env["PROXY_CONSOLE_DATA"]) / "conversations"
        assert registry.is_dir(), "disposable conversation registry unavailable"
        existing = set(registry.glob("*.json"))
        code, _, raw = f.api("/api/v1/drivers/codex/v1/chat/completions", "POST", req)
        assert code == 422, f"unsupported function returned HTTP {code}, expected 422"
        error = json.loads(raw).get("error", {})
        assert error.get("code") == "unsupported_or_invalid_input" and error.get("message"), "missing capability rejection"
        assert set(registry.glob("*.json")) == existing, "unsupported function created a conversation"
        assert not f.events("exec") and not f.events("exchange") and not f.trace, "unsupported function caused dispatch"
        return {"http": code, "error": error["code"], "new_conversations": 0, "rejected_before_exec": True}


def exercise(h, row, negative):
    if row == "PLUGIN-1":
        from auth_plugin import plugin
        return plugin(h, negative)
    if row == "CHAT-1":
        for driver in ("opencode", "codex"):
            if installed(h, driver):
                h.check(driver + " SDK session, transcript and native observer", lambda driver=driver: chat(h, driver, negative))
            else:
                h.block(driver + " SDK session", "installed worker, pinned package or Bun unavailable")
        if negative:
            h.check("incompatible native SDK rejected before session creation", lambda: incompatible(h))
            h.check("foreign actor cannot read shared conversation", lambda: shared(h))
        else:
            h.block("genuine Codex CLI session", "real SDK ran against disposable exec peer, provider use lacks consent")
    if row == "TOOL-1":
        if installed(h, "opencode"):
            h.check("workspace read and permission replay", lambda: tool(h, negative))
            if sys.platform != "linux":
                h.block("kernel-level canary file-open counter", "this platform supplies independent tool-result counts only")
            if installed(h, "codex"):
                h.check("explicit Codex delegation and single executor", lambda: tool(h, negative, True))
            else:
                h.block("explicit delegation", "Codex SDK worker unavailable")
        else:
            h.block("native tool execution", "OpenCode SDK worker unavailable")
        if negative:
            h.check("unsupported function rejected before native admission", lambda: unsupported(h))
            if installed(h, "opencode"):
                h.check("self-referential provider fails without model traffic", lambda: cycle(h))
            h.check("live shared native permission stays with its owner", lambda: foreign_permission(h))

def incompatible(h):
    with fixture(h) as f:
        f.stop_service()
        root = f.root / "incompatible-sdk"
        dep = root / "node_modules/@opencode/sdk"
        dep.mkdir(parents=True)
        (dep / "package.json").write_text(json.dumps({"name": "@opencode/sdk", "version": "1.0.0"}))
        entry = root / "main.mjs"
        # The entry would leave a canary if admission erroneously launched it.
        entry.write_text("await Bun.write(" + json.dumps(str(root / "launched")) + ", 'launched')")
        f.env.update(PROXY_SDK_BUN=h.env.get("PROXY_SDK_BUN", ""), PROXY_SDK_WORKER=str(entry),
                     PROXY_OPENCODE_PROVIDER=json.dumps({"id": "fixture", "model": "fixture-chat", "baseURL":h.upstream + "/v1"}))
        if not f.env["PROXY_SDK_BUN"]:
            raise FileNotFoundError("Bun absent, cannot distinguish SDK rejection")
        f.start_service()
        state = f.connection()
        declared = next(x for x in state["drivers"] if x["id"] == "opencode")
        assert declared["reason"] == "sdk_missing_or_incompatible" and not declared["available"]
        code = f.api("/api/v1/conversations", "POST", {"driver": "opencode"})[0]
        assert code == 503 and not (root / "launched").exists()
        return {"incompatible_version": "1.0.0", "http": code, "worker_launches": 0}


def cycle(h):
    with fixture(h, workers=True) as f:
        f.stop_service()
        f.env["PROXY_OPENCODE_PROVIDER"] = json.dumps({"id": "fixture", "model": "fixture-chat", "baseURL": f.base + "/v1"})
        f.start_service()
        cid = conversation(f, "opencode")
        key = marker("chat-answer")
        turn = start(f, cid, key)
        finish(f, turn, "failed")
        assert not model_requests(h, key) and not f.trace and not f.events("exec")
        return {"visible_terminal": "failed", "independent_upstream_requests": 0}

def foreign_permission(h):
    admin_workspace = (h.root / "shared/admin/workspace").resolve()
    if not admin_workspace.is_dir() or not admin_workspace.is_relative_to(h.root.resolve()):
        raise FileNotFoundError("shared admin disposable workspace unavailable")
    class Shared:
        def api(self, path, method="GET", body=None, **kw):
            return h.http(path, method=method, body=body, base=h.shared, actor="admin", **kw)
    f = Shared()
    key = marker("tool-read")
    canary = admin_workspace / (key + ".txt")
    path = None
    try:
        with canary.open("x") as fd:
            fd.write("value-" + key)
        req = f.api("/api/v1/conversations", "POST", {"driver":"opencode", "model":"fixture-chat"})
        if req[0] == 503:
            raise FileNotFoundError("shared owner lacks native provider fixture")
        cid = status_json(req, 201)["id"]
        path = start(f, cid, key)
        def pending():
            state = view(f, path)
            assert state["state"] not in {"failed", "completed", "indeterminate", "cancelled"}, "permission was not reached"
            return state["permissions"]
        asks = wait_for(pending, 30)
        assert len(asks) == 1
        request = next(iter(asks))
        resource = path + "/permissions/" + request
        code, _, raw = h.http(resource, method="POST", body={"decision":"allow"}, actor="chat", base=h.shared)
        assert code in (403, 404) and key.encode() not in raw
        assert view(f, path)["permissions"] == asks
        assert f.api(resource, "POST", {"decision":"deny"})[0] == 200
        finish(f, path, "cancelled")
        reports = model_requests(h, key)
        assert reports, "independent model recipient did not observe the shared turn"
        assert sum(r["canary_returns"] for r in reports) == 0, "denied canary reached the model"
        assert [e["state"] for e in events(f, path) if e["type"] == "terminal"] == ["cancelled"]
        path = None
        return {"foreign_http":code, "owner_decision":"deny", "terminal":"cancelled", "canary_returns":0}
    finally:
        try:
            if path:
                f.api(path + "/cancel", "POST", {})
        finally:
            canary.unlink(missing_ok=True)
