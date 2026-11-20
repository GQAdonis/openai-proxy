"""Exercise the frozen proxy with a private Linux Secret Service and synthetic recipients."""

import base64
import hashlib
import json
import os
import pathlib
import subprocess
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

w = pathlib.Path("/w")
home = w / "codex"
home.mkdir(mode=0o700)
os.environ.update(
    CODEX_HOME=str(home),
    PROXY_CONSOLE_WORKSPACE="/w/work",
    PROXY_CONSOLE_DATA="/w/state",
    PROXY_BACKEND_CREDENTIAL_CLASS="subscription",
    PATH="/opt/fixture:/usr/bin:/bin",
)
(w / "work").mkdir(mode=0o700)
os.chdir(w / "work")
acct = "cli|" + hashlib.sha256(str(home).encode()).hexdigest()[:16]
(home / "config.toml").write_text('cli_auth_credentials_store = "keyring"\n')
base = ["secret-tool", "lookup", "service", "Codex Auth", "username", acct]


def auth(owner):
    payload = {"exp": int(time.time()) + 3600, "fixture_owner": owner}
    bearer = (
        "fixture."
        + base64.urlsafe_b64encode(json.dumps(payload).encode()).decode().rstrip("=")
        + ".fixture"
    )
    return {
        "auth_mode": "chatgptAuthTokens",
        "tokens": {"access_token": bearer, "account_id": owner},
    }


def install(owner):
    data = auth(owner)
    subprocess.run(
        [
            "secret-tool",
            "store",
            "--label=KBD isolated owner",
            "service",
            "Codex Auth",
            "username",
            acct,
        ],
        input=json.dumps(data),
        text=True,
        check=True,
        timeout=8,
    )
    assert json.loads(subprocess.check_output(base)) == data
    return data["tokens"]["access_token"]


events = []


class Peer(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        owner = self.headers.get("ChatGPT-Account-Id")
        events.append((owner, self.headers.get("Authorization")))
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        for event in [
            {
                "type": "response.output_text.delta",
                "output_index": 0,
                "content_index": 0,
                "delta": "fixture reply",
            },
            {
                "type": "response.completed",
                "response": {
                    "id": "fixture",
                    "status": "completed",
                    "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
                },
            },
        ]:
            self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
        self.wfile.write(b"data: [DONE]\n\n")


s = ThreadingHTTPServer(("127.0.0.1", 0), Peer)
threading.Thread(target=s.serve_forever, daemon=True).start()
url = "http://127.0.0.1:%d/responses" % s.server_port
env = dict(os.environ, CODEX_BACKEND_URL=url)


def get(path, body=None, console=False):
    h = {"Content-Type": "application/json"}
    if console:
        h["Authorization"] = "Bearer " + (w / "state/service-token").read_text().strip()
    req = urllib.request.Request(
        "http://127.0.0.1:8181" + path,
        headers=h,
        data=json.dumps(body).encode() if body is not None else None,
    )
    with urllib.request.urlopen(req, timeout=3) as r:
        return json.loads(r.read())


def conn():
    return get("/api/v1/me/connection", console=True)


def wait(obs, timeout):
    now = time.monotonic()
    while time.monotonic() - now < timeout:
        try:
            v = obs()
            if v:
                return v
        except (OSError, ValueError, urllib.error.HTTPError):
            pass
        time.sleep(0.12)
    raise RuntimeError("fixture timed out waiting for service")


def models():
    native = {
        m["id"]: (m["context_length"], m["max_output_tokens"])
        for m in get("/v1/models")["data"]
    }
    x = subprocess.run(
        ["/opt/fixture/openai-proxy", "setup", "opencode", "--project", "--dry-run"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        timeout=15,
    )
    p = json.loads(x.stdout)
    print("setup", x.returncode, p["status"], str(p.get("reason", ""))[:90], flush=True)
    cfg = p["export"]["providers"]["openai-proxy"]["models"]
    listed = {k: (v["limit"]["context"], v["limit"]["output"]) for k, v in cfg.items()}
    assert x.returncode == 0 and p["status"] == "ready" and native and native == listed
    return len(native)


p = subprocess.Popen(
    [
        "/opt/fixture/openai-proxy",
        "serve",
        "--host",
        "127.0.0.1",
        "--port",
        "8181",
        "--mcp-http-port",
        "8182",
    ],
    env=env,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
try:
    wait(lambda: get("/health"), 8)
    assert conn()["status"] == "signed_out"
    result = {
        "broker": "GNOME Secret Service, private D-Bus and tmpfs login collection",
        "providerConfig": "keyring",
        "setupClient": subprocess.check_output(
            ["/opt/fixture/opencode", "--version"], env=env, text=True
        ).strip(),
        "binary": "c46d15bfedcb9b127d069c799f3b79b1ee5780462c18f91ad9cb0552936c793f",
        "owners": [],
    }
    prior = 0
    for owner in ("owner-a", "owner-b"):
        started = time.monotonic()
        expected = install(owner)

        def current():
            r = conn()
            if (
                r["status"] == "ready"
                and r["source"] == "keyring"
                and r["generation"] > prior
            ):
                return r

        r = wait(current, 29.7)
        print("keyring available", r["generation"], flush=True)
        ans = get(
            "/v1/chat/completions",
            {
                "model": "gpt-5.5",
                "messages": [{"role": "user", "content": "synthetic probe"}],
                "stream": False,
            },
        )
        elapsed = time.monotonic() - started
        assert (
            events[-1] == (owner, "Bearer " + expected)
            and "fixture reply" in json.dumps(ans)
            and elapsed <= 30
            and p.poll() is None
        )
        prompt = {
            "model": "gpt-5.5",
            "messages": [{"role": "user", "content": "fixture"}],
        }
        ag = urllib.request.Request(
            "http://127.0.0.1:8181/ag-ui/stream",
            headers={"Content-Type": "application/json"},
            data=json.dumps(prompt).encode(),
        )
        with urllib.request.urlopen(ag, timeout=4) as answer:
            stream = answer.read()
            assert b"fixture reply" in stream and b"RUN_FINISHED" in stream
        rpc = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "chat_completion", "arguments": prompt},
        }
        mcp = urllib.request.Request(
            "http://127.0.0.1:8182/mcp",
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
            },
            data=json.dumps(rpc).encode(),
        )
        with urllib.request.urlopen(mcp, timeout=4) as answer:
            reply = answer.read()
        assert b"fixture reply" in reply and len(events) == (
            3 if owner == "owner-a" else 6
        )
        assert all(e == (owner, "Bearer " + expected) for e in events[-3:])
        count = models()
        prior = r["generation"]
        result["owners"].append(
            {
                "principal": owner,
                "publicationToRecipientMs": round(elapsed * 1000),
                "generation": prior,
                "sameProcess": True,
                "recipientMatched": True,
                "modelIdsAndLimitsEqual": count,
                "transports": ["REST", "legacy AG-UI", "MCP HTTP"],
            }
        )
    subprocess.run(["secret-tool", "clear", *base[2:]], check=True, timeout=8)
    wait(lambda: conn()["status"] == "signed_out", 30)
    cleared = subprocess.run(
        ["/opt/fixture/openai-proxy", "setup", "opencode", "--project", "--dry-run"],
        env=env,
        capture_output=True,
        timeout=15,
        check=True,
    )
    assert (
        json.loads(cleared.stdout)["export"]["providers"]["openai-proxy"]["models"]
        == {}
    )
    try:
        get("/v1/models")
    except urllib.error.HTTPError as error:
        denied = error.code
    else:
        raise AssertionError("signed-out models unexpectedly available")
    assert denied >= 400
    result["afterKeyringRemoval"] = {
        "apiModelStatus": denied,
        "freshSetupModels": 0,
        "servicePidUnchanged": p.poll() is None,
    }
    assert not (home / "auth.json").exists() and len(events) == 6
    result["fileStoreAbsent"] = True
    result["recipientCalls"] = len(events)
    print("RECEIPT " + json.dumps(result), flush=True)
finally:
    p.terminate()
    p.wait(5)
    s.shutdown()
