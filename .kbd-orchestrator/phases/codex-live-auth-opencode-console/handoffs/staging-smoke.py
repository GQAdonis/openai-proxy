"""Smoke the signed-out local OrbStack release with a single private pairing code on stdin."""

from datetime import datetime, timezone
from html.parser import HTMLParser
from http.client import HTTPConnection
from http.cookies import SimpleCookie
import json
import re
import sys

ORIGIN = "http://127.0.0.1:18181"


class SmokeFailure(Exception):
    pass


def require(ok, check):
    if not ok:
        raise SmokeFailure(check)


def request(method, path, status, *, headers=None, data=None):
    connection = HTTPConnection("127.0.0.1", 18181, timeout=40)
    try:
        connection.request(method, path, data, {"Origin": ORIGIN, **(headers or {})})
        response = connection.getresponse()
        require(
            response.status == status,
            f"{method} {path} returned {response.status}, expected {status}",
        )
        return response.headers, response.read()
    finally:
        connection.close()


class Scripts(HTMLParser):
    def __init__(self):
        super().__init__()
        self.assets = set()

    def handle_starttag(self, tag, attrs):
        src = dict(attrs).get("src") or ""
        if tag == "script" and re.fullmatch(
            r"/ui/assets/[A-Za-z0-9_-]+-[A-Za-z0-9_-]{8,}\.js", src
        ):
            self.assets.add(src)


def smoke():
    token = sys.stdin.buffer.readline(1024).strip()
    require(0 < len(token) < 1024, "pairing code missing or invalid")
    _, health = request("GET", "/health", 200)
    require(json.loads(health).get("status") == "ok", "unexpected health response")
    assets = set()
    for path in ("/ui/", "/ui/settings"):
        head, body = request("GET", path, 200, headers={"Accept": "text/html"})
        require(head.get_content_type() == "text/html", f"{path} content type")
        scripts = Scripts()
        scripts.feed(body.decode())
        require(scripts.assets, f"{path} lacks a hashed script")
        assets.update(scripts.assets)
    for asset in sorted(assets):
        head, body = request("GET", asset, 200)
        require(
            body
            and head.get_content_type() in ("text/javascript", "application/javascript")
            and "immutable" in head.get("Cache-Control", ""),
            "embedded JavaScript response",
        )
    request("GET", "/api/v1/admin/settings", 401)
    head, raw = request(
        "POST",
        "/api/v1/session",
        200,
        headers={"Content-Type": "application/json"},
        data=json.dumps({"token": token.decode()}).encode(),
    )
    pairing = json.loads(raw)
    cookie = SimpleCookie(head.get("Set-Cookie", ""))
    require(
        "oproxy_session" in cookie and pairing.get("csrfToken"),
        "pairing did not establish a session",
    )
    auth = {"Cookie": "oproxy_session=" + cookie["oproxy_session"].value}
    try:
        _, raw = request("GET", "/api/v1/session", 200, headers=auth)
        session = json.loads(raw)
        require(
            (session.get("mode"), session.get("role")) == ("local", "admin"),
            "unexpected session role or mode",
        )
        _, raw = request("GET", "/api/v1/me/connection", 200, headers=auth)
        state = json.loads(raw)
        require(
            state.get("status") == "signed_out",
            "staging credential home is not signed out",
        )
        drivers = {d["id"]: (d["available"], d["reason"]) for d in state["drivers"]}
        require(
            drivers
            == {
                "codex": (True, "installed"),
                "opencode": (False, "scoped_provider_required"),
            },
            "unexpected SDK driver readiness",
        )
        _, raw = request("GET", "/api/v1/admin/settings", 200, headers=auth)
        areas = json.loads(raw)["areas"]
        require(
            any(
                a.get("value", {}).get("mode") == "local"
                and a.get("value", {}).get("actorRole") == "administrator"
                for a in areas
            ),
            "local administrator settings missing",
        )
    finally:
        request(
            "DELETE",
            "/api/v1/session",
            204,
            headers={**auth, "X-CSRF-Token": pairing["csrfToken"]},
        )
    request("GET", "/api/v1/admin/settings", 401, headers=auth)
    return {
        "status": "PASS",
        "origin": ORIGIN,
        "verifiedAt": datetime.now(timezone.utc).isoformat(),
        "health": "ok",
        "settingsIndex": True,
        "embeddedAssets": sorted(assets),
        "adminSettings": 200,
        "anonymousSettings": 401,
        "loggedOutSettings": 401,
        "credentialState": state["status"],
        "driverStates": drivers,
    }


if __name__ == "__main__":
    try:
        print(json.dumps(smoke(), indent=2))
    except SmokeFailure as exc:
        sys.exit(f"Staging smoke failed: {exc}")
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as exc:
        sys.exit(
            "Staging smoke failed during transport or response validation ("
            + type(exc).__name__
            + ")"
        )
