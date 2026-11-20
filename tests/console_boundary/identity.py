"""Boundary cases for signed revision 2. Loaded only by the authorized packet."""
import json
import time
from concurrent.futures import ThreadPoolExecutor

from auth_fixture import atomic, credential, digest, fixture, wait_for


def status_json(call, expected):
    code, _, body = call
    assert code == expected, f"expected HTTP {expected}, received {code}"
    return json.loads(body)


def install(f, doc):
    atomic(f.store, doc)
    state = status_json(f.api("/api/v1/me/connection/reload", "POST", {}), 200)
    assert state["status"] == "ready", "synthetic file login did not become ready"
    return state


def switching(f, doc):
    started = time.monotonic()
    atomic(f.store, doc)
    target = doc["tokens"]
    expected = (target["account_id"], digest(target["access_token"]))

    def delivered():
        code, _, raw = f.chat()
        return (code == 200 and f.trace and f.trace[-1] == expected
                and json.loads(raw)["choices"][0]["message"]["content"] == "fixture reply")
    wait_for(delivered, 2)
    elapsed = time.monotonic() - started
    assert elapsed <= 2, "file commit to observed delivery exceeded two seconds"
    return round(elapsed * 1000)


def auth_one(h, negative):
    def exercise():
        with fixture(h) as f:
            pid = f.proc.pid
            a = credential("owner-a")
            if negative:
                install(f, a)
                f.expect(a)
                original = f.connection()["generation"]
                atomic(f.home / "unrelated.json", credential("owner-b"))
                time.sleep(1)
                f.expect(a)
                assert f.connection()["generation"] == original
                f.store.write_text('{"tokens":')
                wait_for(lambda: f.connection()["status"] == "incomplete_store")
                checkpoint = len(f.trace)
                assert f.chat()[0] >= 400
                assert len(f.trace) == checkpoint, "partial file triggered egress"
                (f.home / "config.toml").write_text('cli_auth_credentials_store = "ephemeral"\n')
                atomic(f.store, credential("owner-b"))
                assert status_json(f.api("/api/v1/me/connection/reload", "POST", {}), 200)["status"] == "unsupported_store"
                assert f.chat()[0] >= 400 and len(f.trace) == checkpoint
                return {"partial_and_unsupported_recipient_calls": 0, "unrelated_generation_unchanged": True}
            assert f.connection()["status"] == "signed_out"
            timings = [switching(f, value) for value in (a, credential("owner-a"), credential("owner-b"))]
            assert f.proc.poll() is None and f.proc.pid == pid
            return {"same_process": True, "recipient_commit_ms": timings, "ordered_owners": ["a", "a", "b"]}
    h.check("native REST recipient identity and file publication", exercise)
    if negative:
        h.block("replaced single-file container mount", "requires a separately mounted container fixture")
    else:
        h.block("native keyring and missed watcher event", "requires an isolated platform keyring and controlled watcher loss")
        from auth_stdio import transport
        for kind in ("mcp", "acp"):
            h.check(kind + " stdio owner switch", lambda kind=kind: transport(h, kind))
        h.block("container credential publication and fresh setup CLI", "native stdio/REST evidence lacks a mounted auth store and client discovery comparison")
        h.block("genuine Codex login", "no separately authorized subscription login")


def auth_two(h, negative):
    def rotate(trigger):
        with fixture(h) as f:
            before = credential("owner-a", managed=True, ttl=30 if trigger == "proactive" else 900)
            after = credential("owner-a", managed=True)
            (f.home / "exchange-mode").write_text("hold-rotate")
            atomic(f.home / "next-generation", after)
            state = install(f, before)
            if trigger == "challenge":
                f.reject_hash = digest(before["tokens"]["access_token"])
            with ThreadPoolExecutor(max_workers=6) as pool:
                readers = [pool.submit(f.chat) for _ in range(3)]
                wait_for(lambda: f.events("exchange"))
                manuals = [pool.submit(f.refresh, state["generation"]) for _ in range(3)]
                (f.home / "release").touch()
                for result in readers:
                    assert result.result(timeout=12)[0] == 200, "eligible request failed after rotation"
                for result in manuals:
                    assert result.result(timeout=12)[0] in (200, 409), "unexpected generation result"
            assert len(f.events("exchange")) == 1
            assert json.loads(f.store.read_text()) == after
            f.expect(after)
            observed = f.connection()
            assert observed["generation"] > state["generation"] and observed["lastRefresh"]
            return {"trigger": trigger, "managed_exchange_count": 1, "committed_freshness": True}
    def fault(mode):
        with fixture(h) as f:
            (f.home / "exchange-mode").write_text(mode)
            a = credential("owner-a", managed=True)
            state = install(f, a)
            updated = status_json(f.refresh(state["generation"]), 200)
            expected = "sign_in_required" if mode == "revoked" else "refresh_unconfirmed"
            assert updated["status"] == expected and updated["lastRefresh"] is None
            assert updated["generation"] == state["generation"]
            assert f.refresh(state["generation"])[0] >= 400
            assert len(f.events("exchange")) == 1 and json.loads(f.store.read_text()) == a
            return {"mode": mode, "status": expected, "exchanges": 1}
    def quota():
        with fixture(h) as f:
            a = credential("owner-a", managed=True)
            install(f, a)
            f.auth_status = 429
            assert f.chat()[0] == 429
            assert f.trace == [("owner-a", digest(a["tokens"]["access_token"]))] and not f.events("exchange")
            return {"http": 429, "recipient_calls": 1, "exchanges": 0}
    def external():
        with fixture(h) as f:
            a = credential("owner-a")
            state = install(f, a)
            assert not state["capabilities"]["refresh"]
            assert f.refresh(state["generation"])[0] >= 400
            f.expect(a)
            assert not f.events("exchange")
            return {"mode": "external", "managed_exchanges": 0}
    def ordinary_key():
        with fixture(h) as f:
            f.stop_service()
            f.env["PROXY_BACKEND_CREDENTIAL_CLASS"] = "api_key"
            f.env["PROXY_CHAT_AUTH"] = "api_key"
            f.start_service()
            state = f.connection()
            assert state["mode"] == "api_key" and not state["capabilities"]["refresh"]
            assert f.refresh(state["generation"])[0] >= 400
            assert f.chat()[0] == 200 and f.trace == [(None, digest(f.env["OPENAI_API_KEY"]))]
            assert not f.events("exchange")
            return {"source": state["source"], "key_owner_confirmed": True, "managed_exchanges": 0}
    if negative:
        for mode in ("metadata", "interrupted", "revoked"):
            h.check("managed exchange " + mode, lambda mode=mode: fault(mode))
        h.check("subscription quota without paid fallback", quota)
    else:
        for trigger in ("proactive", "challenge"):
            h.check(trigger + " overlaps authorized manual rotation", lambda trigger=trigger: rotate(trigger))
        h.check("external token retains its owner", external)
        h.check("ordinary API key uses its own recipient class", ordinary_key)
        h.block("real account managed renewal and embedding billing", "no consented provider session or billing evidence")


def auth_three(h, negative):
    def race(logout):
        with fixture(h, workers=negative) as f:
            a = credential("owner-a", managed=True)
            b = credential("owner-b")
            state = install(f, a)
            cid = None
            if negative:
                created = f.api("/api/v1/conversations", "POST", {"driver": "codex", "model": "gpt-5.5"})
                if created[0] == 503:
                    h.block("old bound conversation", "matching installed worker and Bun unavailable")
                else:
                    cid = status_json(created, 201)["id"]
            (f.home / "exchange-mode").write_text("hold-rotate")
            atomic(f.home / "next-generation", credential("owner-a", managed=True))
            with ThreadPoolExecutor(max_workers=2) as pool:
                turn = pool.submit(f.chat, "KBD_HOLD")
                assert f.held_request.wait(4), "A turn did not reach independent recipient"
                renewal = pool.submit(f.refresh, state["generation"])
                wait_for(lambda: f.events("exchange"))
                if logout:
                    f.store.unlink()
                else:
                    atomic(f.store, b)
                expected = "signed_out" if logout else "ready"
                next_state = status_json(f.api("/api/v1/me/connection/reload", "POST", {}), 200)
                assert next_state["status"] == expected and next_state["generation"] > state["generation"]
                mark = len(f.trace)
                # Returning from refresh before releasing the old writer is the
                # observable fence. A live writer would overwrite auth.json.
                renewal.result(timeout=5)
                (f.home / "release").touch()
                f.release_request.set()
                renewal.result(timeout=10)
                from runtime import ps_stamp
                old_pid = f.events("exchange")[0]["pid"]
                wait_for(lambda: not ps_stamp(old_pid), 5)
                turn.result(timeout=10)
                if logout:
                    assert not f.store.exists() and f.chat()[0] >= 400
                    assert f.trace[mark:] == []
                else:
                    assert json.loads(f.store.read_text()) == b, "old exchange changed the authoritative store"
                    f.expect(b)
                    assert f.trace[mark:] == [("owner-b", digest(b["tokens"]["access_token"]))]
                if cid:
                    denial = f.api(f"/api/v1/conversations/{cid}/turns", "POST",
                                   {"prompt": "fixture resume", "idempotencyKey": "old-owner"})
                    assert denial[0] in (403, 404) and not f.events("exec")
            return {"old_exchanges": len(f.events("exchange")), "post_commit_owner": "signed-out" if logout else "b",
                    "old_turn_recipient_calls": mark}
    for logout in (False, True):
        h.check(("logout" if logout else "account switch") + " during held exchange", lambda logout=logout: race(logout))
    if negative:
        h.block("genuine obsolete grant publication", "synthetic delayed issuer cannot establish provider-side save semantics")


def run(h, row, negative):
    key = row if isinstance(row, str) else row["id"]
    if key == "AUTH-1":
        return auth_one(h, negative)
    if key == "AUTH-2":
        return auth_two(h, negative)
    if key == "AUTH-3":
        return auth_three(h, negative)
    from auth_native import exercise
    return exercise(h, key, negative)
