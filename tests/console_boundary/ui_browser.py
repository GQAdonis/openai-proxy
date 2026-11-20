"""Browser journeys through the frozen release and its native, loopback-backed worker."""
import contextlib
from datetime import datetime
from importlib.metadata import version
import json
import os
import sys
from pathlib import Path
import re
import shutil
import time
from uuid import uuid4

from admin_common import MissingPeer, require
import ui_upstream as fixture

STREAM = "/api/v1/ag-ui/stream"


def rows(h):
    path = h.root / "upstream-observations.jsonl"
    return [json.loads(s) for s in path.read_text().splitlines() if s] if path.exists() else []


def action_count(h, marker):
    return sum(r.get("surfaceAction") is True and not r.get("disconnected") and marker in r.get("markers", [])
               for r in rows(h))


def observed_text(page, value):
    page.get_by_text(value, exact=True).first.wait_for(timeout=35000)


def tab_to(page, target):
    for _ in range(90):
        if target.evaluate("el => el === document.activeElement"):
            return
        page.keyboard.press("Tab")
    raise AssertionError("control unreachable in sequential keyboard navigation")


@contextlib.contextmanager
def playwright_driver(h, start):
    # The Playwright Node helper also needs a credential-free process environment.
    cache = Path.home() / ("Library/Caches" if sys.platform == "darwin" else ".cache") / "ms-playwright"
    private = {k: h.env[k] for k in ("HOME", "PATH", "LANG", "TMPDIR", "XDG_CACHE_HOME",
                                      "XDG_CONFIG_HOME", "XDG_DATA_HOME") if k in h.env}
    private["PLAYWRIGHT_BROWSERS_PATH"] = os.environ.get("PLAYWRIGHT_BROWSERS_PATH", str(cache))
    previous = dict(os.environ)
    try:
        os.environ.clear()
        os.environ.update(private)
        pw = start().start()
    finally:
        os.environ.clear()
        os.environ.update(previous)
    try:
        yield pw, private
    finally:
        pw.stop()


@contextlib.contextmanager
def browser(h, base, actor, pairing_path=None):
    try:
        from playwright.sync_api import sync_playwright, Error
    except ImportError as exc:
        raise MissingPeer("Python Playwright driver unavailable") from exc
    with playwright_driver(h, sync_playwright) as (pw, private):
        try:
            cached = Path(private["PLAYWRIGHT_BROWSERS_PATH"])
            alternatives = [Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
                            Path("/Applications/Chromium.app/Contents/MacOS/Chromium")]
            system = shutil.which("chromium", path=private["PATH"]) or shutil.which("google-chrome", path=private["PATH"])
            if system:
                alternatives.append(Path(system))
            executable = None
            if not any(cached.glob("chromium_headless_shell-*")):
                executable = next((str(p) for p in alternatives if p.is_file()), None)
            chrome = pw.chromium.launch(env=private, executable_path=executable, headless=True,
                                        args=["--disable-background-networking", "--disable-component-update"])
        except Error as exc:
            raise MissingPeer("installed Chromium unavailable to the isolated Playwright driver") from exc
        try:
            with logged_in(h, chrome, base, actor, pairing_path) as page:
                yield page
        finally:
            chrome.close()


@contextlib.contextmanager
def logged_in(h, chrome, base, actor, pairing_path=None):
    ctx = chrome.new_context(service_workers="block")
    try:
        # Guard every browser request, including redirects and all subresources.
        ctx.route("**/*", lambda r: r.continue_() if r.request.url.startswith(base + "/")
                  else r.abort("blockedbyclient"))
        page = ctx.new_page()
        page.set_default_timeout(15000)
        page.goto(base + "/ui/", wait_until="domcontentloaded")
        access = page.get_by_label("Console access code")
        access.wait_for()
        require(access.evaluate("el => el === document.activeElement"), "pairing field did not receive focus")
        if actor == "local" or pairing_path is not None:
            path = pairing_path or (Path(h.env["PROXY_CONSOLE_DATA"]) / "pairing-token")
            require(path.resolve().is_relative_to(h.root.resolve()), "pairing file outside disposable root")
            token = path.read_text().strip()
        elif actor == "image":
            token = getattr(h, "image_pairing", None)
            if not token:
                raise MissingPeer("disposable final-image pairing code unavailable")
        else:
            token = h.keys[actor]
        access.fill(token)
        page.keyboard.press("Enter")
        page.get_by_role("heading", name="Conversation", exact=True).wait_for()
        h.note("browser runtime " + actor, {"chromium": chrome.version, "playwright": version("playwright")})
        yield page
    finally:
        ctx.close()


def nav(h, base, actor):
    with browser(h, base, actor) as page:
        link = page.get_by_role("link", name="Settings", exact=True)
        tab_to(page, link)
        page.keyboard.press("Enter")
        page.get_by_role("heading", name="Settings", exact=True).wait_for()
        require(page.url == base + "/ui/settings", "keyboard navigation missed settings URL")
        page.reload(wait_until="domcontentloaded")
        page.get_by_role("heading", name="My connection").wait_for()
        field = page.get_by_role("searchbox", name="Find a setting")
        tab_to(page, field)
        field.fill("My connection")
        nav = page.get_by_role("navigation", name="Settings areas")
        require(nav.get_by_role("button").count() == 1, "settings filter did not narrow the navigator")
        chat = page.get_by_role("link", name="Chat", exact=True)
        tab_to(page, chat)
        page.keyboard.press("Enter")
        page.get_by_role("heading", name="Conversation", exact=True).wait_for()
        require(page.url == base + "/ui/", "keyboard return did not navigate to chat")
        details = page.request.get(base + "/api/v1/me/connection").json()
        driver = page.get_by_label("Driver", exact=True)
        for row in details["drivers"]:
            option = driver.locator("option").filter(has_text=row["id"])
            require(option.count() == 1 and option.is_disabled() == (not row["available"]),
                    "driver selector disagrees with actual service capability")
        return {"deployment": actor, "urls": ["/ui/", "/ui/settings"],
                "drivers": {d["id"]: d["available"] for d in details["drivers"]}}


def prepare(page):
    field = page.get_by_label("Driver", exact=True)
    item = field.locator('option[value="opencode"]')
    if not item.count() or item.is_disabled():
        raise MissingPeer("OpenCode native worker unavailable on this packaged service")
    field.select_option("opencode")
    models = page.get_by_label("Model", exact=True)
    options = models.locator("option").all_text_contents()
    if not options:
        raise MissingPeer("fixture model absent from the native provider selector")
    # The synthetic OpenCode model is appended to the real catalog by the service.
    models.select_option(options[-1])


def send(page, marker):
    composer = page.get_by_label("Message", exact=True)
    composer.fill(marker)
    with page.expect_response(lambda r: r.request.method == "POST" and r.url.endswith("/turns"),
                              timeout=30000) as admitted:
        composer.press("Control+Enter")
    res = admitted.value
    require(res.status == 202, f"native admission returned HTTP {res.status}")
    return res.url.split("/")[-2], res.json()["id"]


def sse(response):
    require(response.status == 200, f"AG-UI projection HTTP {response.status}")
    require("text/event-stream" in response.headers.get("content-type", ""), "non-SSE AG-UI response")
    return [json.loads(line[5:].strip()) for line in response.text().splitlines() if line.startswith("data:")]


def wire_body(cid, tid):
    return {"threadId": cid, "runId": tid, "tools": [], "context": [], "messages": [],
            "state": {}, "forwardedProps": {}}


def post(page, base, body, csrf=True):
    headers = {"Origin": base}
    if csrf:
        headers["X-CSRF-Token"] = page.request.get(base + "/api/v1/session").json()["csrfToken"]
    return page.request.post(base + STREAM, data=body, headers=headers, timeout=30000)


def project_trace(events, cid, tid, marker):
    require(events and events[0].get("type") == "RUN_STARTED" and
            (events[0].get("threadId"), events[0].get("runId")) == (cid, tid), "incorrect AG-UI run start")
    require(events[-1].get("type") == "RUN_FINISHED" and
            sum(e["type"] in ("RUN_FINISHED", "RUN_ERROR") for e in events) == 1, "incorrect AG-UI terminal")
    snapshots = [e for e in events if e.get("type") == "ACTIVITY_SNAPSHOT"]
    require(snapshots and len({e["messageId"] for e in snapshots}) == 1, "replacement message identity changed")
    require(all(e.get("activityType") == "a2ui-surface" and e.get("replace") is True for e in snapshots),
            "AG-UI activity envelope mismatch")
    require([e.get("content", {}).get("a2ui_operations") for e in snapshots] == fixture.snapshots(marker),
            "emitted create, data update, duplicate or delete differs from fixture")
    return snapshots[0]["messageId"]


def rendered(page):
    control = page.get_by_role("button", name=fixture.NAME, exact=True)
    control.wait_for(timeout=35000)
    require(control.count() == 1 and control.inner_text() == "Final choice", "updated component lost its caption or duplicated")
    require(not page.get_by_text("Temporary choice", exact=True).count(), "deleted surface is still visible")
    cdp = page.context.new_cdp_session(page)
    try:
        nodes = cdp.send("Accessibility.getFullAXTree")["nodes"]
        matching = [n for n in nodes if not n.get("ignored") and n.get("role", {}).get("value") == "button"
                    and n.get("name", {}).get("value") == fixture.NAME]
        require(len(matching) == 1 and matching[0].get("description", {}).get("value") == fixture.DESCRIPTION,
                "Chromium accessibility tree lacks the expected button name or description")
    finally:
        cdp.detach()
    return control


def surface(h, page, base, marker):
    prepare(page)
    # Hold only the redundant REST replay until the AG-UI client first renders the
    # component. Both responses originate in the packaged service.
    held = []
    path = base + "/api/v1/conversations/*/turns/*/events"
    page.route(path, lambda route: held.append(route))
    try:
        with page.expect_response(lambda r: r.url == base + STREAM, timeout=30000) as stream:
            cid, tid = send(page, marker)
        first = sse(stream.value)
        mid = project_trace(first, cid, tid, marker)
        page.get_by_text("Final choice", exact=True).wait_for(timeout=30000)
        require(held, "REST replay was not held while exercising AG-UI")
    finally:
        page.unroute(path)
        for route in held:
            route.continue_()
    observed_text(page, fixture.PLAIN)
    observed_text(page, "Complete")
    control = rendered(page)
    again = sse(post(page, base, wire_body(cid, tid)))
    require(project_trace(again, cid, tid, marker) == mid and first == again, "replayed projection differs")
    require(rendered(page).count() == 1, "duplicate projection changed visible control count")
    h.note("typed UI projection " + marker, {"events": first, "replayMessage": mid,
           "replayEventCount": len(again), "tree": {"role": "button", "name": fixture.NAME,
           "description": fixture.DESCRIPTION}, "browserVisibleCaption": control.inner_text()})
    return cid, tid, control


def model_turn(h, base, actor, partial=False):
    marker = ("kbd-ui-partial-" if partial else "kbd-ui-stop-") + actor
    with browser(h, base, actor) as page:
        prepare(page)
        cid, tid = send(page, marker)
        if partial:
            page.get_by_text(re.compile(re.escape(fixture.PARTIAL))).first.wait_for(timeout=30000)
            page.get_by_text(re.compile("Reply interrupted after partial output")).wait_for(timeout=35000)
        else:
            page.get_by_text(re.compile(re.escape(fixture.STOP))).first.wait_for(timeout=30000)
            stop = page.get_by_role("button", name="Stop reply")
            tab_to(page, stop)
            page.keyboard.press("Enter")
            observed_text(page, "Stopped")
        turn = page.request.get(base + f"/api/v1/conversations/{cid}/turns/{tid}").json()
        expected = "failed" if partial else "cancelled"
        require(turn["state"] == expected and (fixture.PARTIAL if partial else fixture.STOP) in turn["text"],
                "native terminal or saved partial text differs from browser outcome")
        require(page.get_by_label("Message", exact=True).evaluate("el => el === document.activeElement"),
                "composer focus was lost after terminal")
        if not partial:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if any(r.get("disconnected") and marker in r.get("markers", []) for r in rows(h)):
                    break
                time.sleep(0.1)
            else:
                raise AssertionError("native stop did not interrupt the upstream response")
        return {"actor": actor, "terminal": expected, "preserved": True, "turn": tid}


def action_journey(h, negative):
    actor = "chat"
    base = h.shared
    marker = "kbd-surface-negative" if negative else "kbd-surface-positive"
    with browser(h, base, actor) as page:
        before = action_count(h, marker)
        cid, tid, control = surface(h, page, base, marker)
        action = {"name": "choose", "surfaceId": fixture.SURFACE, "sourceComponentId": "root",
                  "timestamp": "2026-01-01T00:00:00Z", "context": {"item": marker}}

        def envelope(a):
            data = wire_body(cid, tid)
            data["forwardedProps"] = {"a2uiAction": {"userAction": a, "idempotencyKey": str(uuid4())}}
            return data

        if negative:
            with logged_in(h, page.context.browser, base, "chat2") as other:
                foreign_marker = "kbd-surface-foreign"
                foreign_cid, _, foreign_control = surface(h, other, base, foreign_marker)
                require(foreign_cid != cid and foreign_control.count() == 1, "two owners did not receive distinct runs")
                result = post(other, base, wire_body(cid, tid))
                require(result.status in (403, 404), "foreign projection returned a run")
                require(post(other, base, envelope(action)).status in (403, 404), "foreign action admitted")
            defaults = h.root / "shared/state/defaults.json"
            before_host = defaults.read_bytes()
            for altered in [dict(action, context={"item": "other"}), dict(action, name="setDefaultModel"),
                            dict(action, surfaceId=fixture.TEMP)]:
                result = post(page, base, envelope(altered))
                require(result.status in (400, 403, 404, 409, 422), "invalid action accepted")
            require(post(page, base, envelope(action), csrf=False).status == 403, "missing CSRF was accepted")
            require(action_count(h, marker) == before and rendered(page).count() == 1, "rejected operation had an effect")
            require(before_host == defaults.read_bytes(), "host settings changed after rejected conversation action")
            return {"thread": cid, "rejectedEffects": 0, "hostWrites": 0, "foreignThread": foreign_cid}

        tab_to(page, control)
        with page.expect_response(lambda r: r.url == base + STREAM, timeout=30000) as received:
            page.keyboard.press("Enter")
        response = received.value
        req = response.request.post_data_json
        live = req["forwardedProps"]["a2uiAction"]["userAction"]
        require(set(live) == set(action), "unexpected renderer action fields")
        require(datetime.fromisoformat(live["timestamp"].replace("Z", "+00:00")).utcoffset() is not None,
                "renderer sent an unzoned action timestamp")
        require(dict(live, timestamp=action["timestamp"]) == action and
                (req["threadId"], req["runId"]) == (cid, tid), "action lost scoped data or control identity")
        events = sse(response)
        observed_text(page, fixture.ACTION_ACK)
        require(events[-1]["type"] == "RUN_FINISHED" and events[0]["runId"] != tid, "action run did not complete")
        count = action_count(h, marker)
        require(count == before + 1, "receiver count differs from one scoped action")
        replay = sse(post(page, base, req))
        require(replay[0]["runId"] == events[0]["runId"], "replayed action was admitted twice")
        require(post(page, base, envelope(live)).status in (403, 409), "new-key replay was accepted")
        require(action_count(h, marker) == count, "duplicate action reached receiver")
        h.note("surface projection and action", {"thread": cid, "turn": tid, "result": events[0]["runId"],
               "receiverDelta": count-before, "observedAction": live, "name": fixture.NAME, "description": fixture.DESCRIPTION})
        return {"thread": cid, "turn": tid, "receiverDelta": 1}


def invalid_wire(h, kind):
    marker = "kbd-surface-text" if kind == "text" else "kbd-surface-bad-" + kind
    with browser(h, h.shared, "chat") as page:
        prepare(page)
        guard_cid, _ = send(page, "kbd-surface-guard-" + kind)
        observed_text(page, "Preserved sibling surface")
        observed_text(page, "Complete")
        cid, tid = send(page, marker)
        require(cid == guard_cid, "negative activity missed existing conversation")
        path = f"/api/v1/conversations/{cid}/turns/{tid}"
        observed = sse(post(page, h.shared, wire_body(cid, tid)))
        require(not any(e["type"] == "ACTIVITY_SNAPSHOT" for e in observed), "invalid activity reached typed stream")
        detail = page.request.get(h.shared + path).json()
        require(detail.get("surfaces") == [], "invalid activity committed a surface")
        if kind == "text":
            page.get_by_text(re.compile("ACTIVITY_SNAPSHOT")).wait_for()
        require(page.get_by_role("button", name="Untrusted fixture control").count() == 0, "invalid control rendered")
        require(action_count(h, marker) == 0, "invalid fixture reached action receiver")
        # Subsequent valid activity and text traverse the very same conversation.
        observed_text(page, "Preserved sibling surface")
        page.get_by_role("button", name="Stop reply").wait_for(state="hidden", timeout=35000)
        recovery = "kbd-surface-recover-" + kind
        cid2, tid2 = send(page, recovery)
        require(cid2 == cid, "fixture did not recover on the same owned conversation")
        project_trace(sse(post(page, h.shared, wire_body(cid, tid2))), cid2, tid2, recovery)
        rendered(page)
        observed_text(page, fixture.PLAIN)
        observed_text(page, "Preserved sibling surface")
        return {"input": kind, "oldTurn": tid, "recoveredTurn": tid2, "effects": 0}
