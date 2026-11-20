"""One installed service, credential writer and independent loopback Responses recipient."""
import base64
import hashlib
import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def wait_for(predicate, seconds=6):
    end = time.monotonic() + seconds
    while True:
        found = predicate()
        if found:
            return found
        if time.monotonic() >= end:
            raise AssertionError("observation missed its deadline")
        time.sleep(.04)


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def credential(owner, managed=False, ttl=900):
    payload = {"exp": int(time.time()) + ttl, "nonce": secrets.token_hex(10)}
    token = "fixture." + base64.urlsafe_b64encode(json.dumps(payload).encode()).decode().rstrip("=") + ".fixture"
    tokens = {"account_id": owner, "access_token": token}
    if managed:
        tokens["refresh_token"] = secrets.token_hex(20)
    return {"auth_mode": "chatgpt" if managed else "chatgptAuthTokens", "tokens": tokens}


def atomic(path, doc):
    with tempfile.NamedTemporaryFile(dir=path.parent, mode="w", delete=False) as stream:
        json.dump(doc, stream)
    Path(stream.name).replace(path)


class Fixture:
    def __init__(self, h, root, workers=False):
        self.h = h
        self.root = root
        self.home = root / "codex"
        self.home.mkdir()
        self.store = self.home / "auth.json"
        (self.home / ".synthetic-owner").touch()
        (self.home / "exchange-mode").write_text("metadata")
        self.workspace = root / "workspace"
        self.workspace.mkdir()
        self.trace = []
        self.auth_status = 200
        self.reject_hash = None
        self.held_request = threading.Event()
        self.release_request = threading.Event()
        fixture = self

        class Capture(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                if self.path != "/responses":
                    self.send_error(404)
                    return
                bearer = self.headers.get("Authorization", "").removeprefix("Bearer ")
                marker = (self.headers.get("chatgpt-account-id"), digest(bearer))
                fixture.trace.append(marker)
                status = 401 if fixture.reject_hash == marker[1] else fixture.auth_status
                if status != 200:
                    self.send_response(status)
                    if status == 401:
                        self.send_header("WWW-Authenticate", 'Bearer error="invalid_token"')
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                if b"KBD_HOLD" in body:
                    fixture.held_request.set()
                    fixture.release_request.wait(5)
                frames = [{"type": "response.output_text.delta", "output_index": 0, "content_index": 0,
                           "delta": "fixture reply"}, {"type": "response.completed",
                           "response": {"id": "fixture", "model": "gpt-5.5", "status": "completed"}}]
                self.wfile.write("".join("data: " + json.dumps(frame) + "\n\n" for frame in frames).encode())

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Capture)
        self.listener = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.proc = None
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            self.port = reservation.getsockname()[1]
        self.base = "http://127.0.0.1:" + str(self.port)
        self.owner = root / "codex-peer"
        # Python itself may reside outside the app-server's sanitized PATH.
        self.owner.write_bytes(("#!" + sys.executable + "\n").encode() +
                               Path(__file__).with_name("auth_owner.py").read_bytes())
        self.owner.chmod(0o700)
        self.env = {"HOME": str(root), "CODEX_HOME": str(self.home), "PATH": "/usr/bin:/bin",
                    "XDG_CONFIG_HOME": str(root / "config"), "XDG_DATA_HOME": str(root / "data"),
                    "PROXY_CONSOLE_DATA": str(root / "console"), "PROXY_CONSOLE_WORKSPACE": str(self.workspace),
                    "PROXY_CONSOLE_TOKEN": h.keys["local"], "PROXY_CODEX_BINARY": str(self.owner),
                    "PROXY_BACKEND_CREDENTIAL_CLASS": "subscription", "OPENAI_API_KEY": secrets.token_hex(25),
                    "HTTP_PROXY": "http://127.0.0.1:1", "HTTPS_PROXY": "http://127.0.0.1:1",
                    "ALL_PROXY": "http://127.0.0.1:1", "NO_PROXY": "127.0.0.1,localhost",
                    "PROXY_SDK_WORKER": str(root / "absent-worker")}
        if workers:
            self.env["PROXY_OPENCODE_PROVIDER"] = json.dumps({
                "id": "fixture", "model": "fixture-chat", "baseURL": h.upstream + "/v1"})
            self.env["PROXY_SDK_WORKER"] = str(h.root / "install/lib/openai-proxy/workers/main.mjs")
            if h.env.get("PROXY_SDK_BUN"):
                self.env["PROXY_SDK_BUN"] = h.env["PROXY_SDK_BUN"]

    def start(self):
        self.started_at = time.monotonic()
        self.listener.start()
        self.start_service()

    def start_service(self):
        url = "http://127.0.0.1:%d/responses" % self.server.server_port
        self.proc = subprocess.Popen([str(self.h.binary), "serve", "--host", "127.0.0.1",
                                      "--port", str(self.port), "--backend-url", url],
                                     env=self.env, cwd=self.workspace, start_new_session=True, stdin=subprocess.DEVNULL,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        from runtime import ps_stamp
        self.h.state.setdefault("children", []).append({"pid": self.proc.pid, "name": "synthetic-auth",
                                                       "stamp": ps_stamp(self.proc.pid)})
        self.h.save()
        def ready():
            assert self.proc.poll() is None, "installed service exited during startup"
            try:
                return self.h.http("/health", base=self.base)[0] == 200
            except OSError:
                return False
        wait_for(ready, 12)

    def stop_service(self):
        if self.proc is not None and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(6)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()

    def stop(self):
        self.release_request.set()
        self.stop_service()
        if self.listener.is_alive():
            self.server.shutdown()
            self.listener.join(3)
        self.server.server_close()
        self.h.note(self.root.name, {
            "argv": [str(self.h.binary), "serve", "--host", "127.0.0.1", "--port", str(self.port),
                     "--backend-url", "http://127.0.0.1:%d/responses" % self.server.server_port],
            "fixtureRecipient": [{"account": a, "bearerSha256": b} for a, b in self.trace],
            "controlledPeerExchanges": len(self.events("exchange")), "controlledPeerExecs": len(self.events("exec")),
            "elapsedSeconds": round(time.monotonic() - self.started_at, 3),
            "exit": self.proc.returncode if self.proc else None})

    def api(self, path, method="GET", body=None, **kw):
        return self.h.http(path, method=method, body=body, base=self.base, **kw)

    def chat(self, prompt="echo fixture"):
        return self.api("/v1/chat/completions", "POST", {"model": "gpt-5.5",
                        "messages": [{"role": "user", "content": prompt}], "stream": False})

    def connection(self):
        code, _, raw = self.api("/api/v1/me/connection")
        assert code == 200, f"connection HTTP {code}"
        return json.loads(raw)

    def refresh(self, generation):
        return self.api("/api/v1/me/connection/refresh", "POST", {"generation": generation})

    def events(self, kind):
        log = self.home / "observer.jsonl"
        return [v for line in log.read_text().splitlines() if (v := json.loads(line))["kind"] == kind] if log.exists() else []

    def expect(self, doc):
        code, _, raw = self.chat()
        assert code == 200, f"chat HTTP {code}"
        assert json.loads(raw)["choices"][0]["message"]["content"] == "fixture reply"
        t = doc["tokens"]
        assert self.trace[-1] == (t["account_id"], digest(t["access_token"])), "recipient saw different identity"


@contextmanager
def fixture(h, workers=False):
    with tempfile.TemporaryDirectory(prefix="auth-case-", dir=h.root) as folder:
        f = Fixture(h, Path(folder), workers)
        try:
            f.start()
            yield f
        finally:
            f.stop()
