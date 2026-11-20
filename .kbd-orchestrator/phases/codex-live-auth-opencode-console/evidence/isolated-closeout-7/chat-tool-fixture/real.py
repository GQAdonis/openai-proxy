"""Genuine offline Codex Responses peer and public REST replay/cancellation probe."""

import base64
import ctypes
import datetime
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from auth_fixture import Fixture, atomic
from auth_native import conversation, durable_id, events, finish, start


def chat(h):
    requests = []
    closed = threading.Event()
    entered = threading.Event()
    account = "00000000-0000-4000-8000-000000000002"
    b64 = (
        lambda v: base64.urlsafe_b64encode(json.dumps(v).encode()).rstrip(b"=").decode()
    )
    jwt = (
        b64({"alg": "none", "typ": "JWT"})
        + "."
        + b64(
            {
                "exp": int(time.time() + 7200),
                "sub": "synthetic",
                "email": "fixture@example.invalid",
                "https://api.openai.com/auth": {
                    "chatgpt_plan_type": "pro",
                    "chatgpt_account_id": account,
                },
            }
        )
        + ".ZmFrZQ"
    )

    class Peer(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *a):
            pass

        def do_GET(self):
            self.send_error(426 if self.headers.get("Upgrade") else 404)

        def do_POST(self):
            if (
                self.path != "/v1/responses"
                or self.headers.get("Authorization") != "Bearer " + jwt
            ):
                self.send_error(403)
                return
            if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
                b = b""
                while True:
                    n = int(self.rfile.readline().split(b";")[0].strip(), 16)
                    assert 0 <= n <= 2_000_000 - len(b)
                    if not n:
                        self.rfile.readline()
                        break
                    b += self.rfile.read(n)
                    self.rfile.readline()
            else:
                size = int(self.headers["Content-Length"])
                assert 0 <= size <= 2_000_000
                b = self.rfile.read(size)
            if self.headers.get("Content-Encoding") == "zstd":
                lib = ctypes.CDLL("libzstd.so.1")
                out = ctypes.create_string_buffer(2000000)
                n = lib.ZSTD_decompress(out, len(out), b, len(b))
                assert 0 <= n <= len(out)
                b = out.raw[:n]
            doc = json.loads(b)
            inputs = [x for x in doc["input"] if x.get("role") == "user"]
            prompt = json.dumps(inputs[-1])
            held = "KBD_REAL_HOLD" in prompt
            assert held or "KBD_REAL_ACK" in prompt
            requests.append(
                dict(
                    held=held,
                    model=doc["model"],
                    authMatched=True,
                    previousAnswerInContext="KBD_REAL_RESULT" in b.decode(),
                    accountMatched=self.headers.get("chatgpt-account-id") == account,
                )
            )
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True

            def frame(t, **d):
                self.wfile.write(
                    (
                        "event: "
                        + t
                        + "\ndata: "
                        + json.dumps({"type": t, **d})
                        + "\n\n"
                    ).encode()
                )
                self.wfile.flush()

            frame("response.created", response={"id": "resp-proof"})
            if held:
                entered.set()
                try:
                    for i in range(200):
                        self.wfile.write(b": hold\n\n")
                        self.wfile.flush()
                        time.sleep(0.1)
                except (BrokenPipeError, ConnectionResetError):
                    closed.set()
                return
            item = {
                "type": "message",
                "role": "assistant",
                "id": "msg-proof",
                "content": [{"type": "output_text", "text": "KBD_REAL_RESULT"}],
            }
            frame("response.output_item.done", item=item)
            frame(
                "response.completed",
                response={
                    "id": "resp-proof",
                    "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8},
                },
            )

    server = ThreadingHTTPServer(("127.0.0.1", 43195), Peer)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    r = h.root / "actual"
    r.mkdir()
    f = Fixture(h, r, workers=True)
    f.env.pop("OPENAI_API_KEY", None)
    f.env["PROXY_CODEX_BINARY"] = "/opt/proxy/codex"
    doc = {
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": jwt,
            "access_token": jwt,
            "refresh_token": "synthetic-unused",
            "account_id": account,
        },
        "last_refresh": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    atomic(f.store, doc)
    try:
        f.start()
        cid = conversation(f, "codex")
        p = start(f, cid, "KBD_REAL_ACK", "ack-once")
        t = finish(f, p)
        tr = events(f, p)
        assert (
            t["text"] == "KBD_REAL_RESULT"
            and sum(e["type"] == "terminal" for e in tr) == 1
        )
        assert "".join(e["text"] for e in tr if e["type"] == "text") == t["text"]
        assert (
            events(f, p, 1) == tr[1:]
            and start(f, cid, "KBD_REAL_ACK", "ack-once") == p
            and len(requests) == 1
        )
        native = durable_id(f, cid)
        f.stop_service()
        f.start_service()
        assert events(f, p) == tr and durable_id(f, cid) == native
        stopped = start(f, cid, "KBD_REAL_HOLD", "hold-once")
        assert entered.wait(30), "held turn never reached recipient"
        assert f.api(stopped + "/cancel", "POST", {})[0] == 200
        finish(f, stopped, "cancelled")
        assert closed.wait(8)
        assert sum(e["type"] == "terminal" for e in events(f, stopped)) == 1
        assert f.api(stopped + "/cancel", "POST", {})[0] == 200
        rollout = list(f.home.glob("sessions/**/*.jsonl"))
        assert len(rollout) == 1
        assert any(
            (x := json.loads(line))["type"] == "session_meta"
            and x["payload"]["id"] == native
            for line in rollout[0].read_text().splitlines()
        )
        assert len(requests) == 2 and all(x["accountMatched"] for x in requests)
        assert requests[1]["previousAnswerInContext"]
        return dict(
            driver="codex",
            cli="0.169.0",
            successfulReply=True,
            replayedAfterCursor=True,
            serviceRestarted=True,
            nativeSessionMatched=True,
            resumedHistoryObserved=requests[-1]["previousAnswerInContext"],
            cancelTerminalCount=1,
            recipientSocketClosed=True,
            admittedRequests=len(requests),
            recipient=requests,
        )
    finally:
        f.stop()
        server.shutdown()
        server.server_close()
