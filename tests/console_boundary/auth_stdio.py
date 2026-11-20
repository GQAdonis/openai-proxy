"""Reuse the packet's external stdio client for file credential publication."""
import time
from admin_rpc import process
from auth_fixture import atomic, credential, digest, fixture


def transport(h, kind):
    with fixture(h) as f:
        a, b = credential("owner-a"), credential("owner-b")
        atomic(f.store, a)
        args = [h.binary, "serve", "--" + kind + "-stdio", "--backend-url",
                "http://127.0.0.1:%d/responses" % f.server.server_port]
        with process(h, args, f.workspace, f.env) as peer:
            if kind == "mcp":
                peer.call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                          "clientInfo": {"name": "identity-observer", "version": "1"}})
                peer.send("notifications/initialized", {})
                def request():
                    reply = peer.call("tools/call", {"name": "chat_completion", "arguments": {
                        "model": "gpt-5.5", "messages": [{"role": "user", "content": "fixture"}]}})
                    assert not reply.get("isError") and "fixture reply" in str(reply["content"])
            else:
                peer.call("initialize", {"protocolVersion": 1, "clientCapabilities": {}})
                def request():
                    session = peer.call("session/new", {"cwd": str(f.workspace), "mcpServers": []})["sessionId"]
                    result = peer.call("session/prompt", {"sessionId": session, "prompt": [{"type": "text", "text": "fixture"}]})
                    assert result["stopReason"] == "end_turn"
                    assert any("fixture reply" in str(n.get("params", {}).get("update", {}))
                               for n in peer.notifications if n.get("params", {}).get("sessionId") == session)
            request()
            assert f.trace[-1] == ("owner-a", digest(a["tokens"]["access_token"]))
            started = time.monotonic()
            atomic(f.store, b)
            while True:
                request()
                if f.trace[-1] == ("owner-b", digest(b["tokens"]["access_token"])):
                    break
                assert time.monotonic() - started < 2, "stdio credential publication deadline missed"
                time.sleep(.05)
            elapsed = time.monotonic() - started
            assert elapsed <= 2 and peer.child.poll() is None
            return {"transport": kind, "unchangedPid": peer.child.pid, "ms": round(elapsed * 1000)}
