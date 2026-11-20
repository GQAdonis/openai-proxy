"""Loopback text-only model used to observe a packaged OpenCode SDK turn's cancellation."""

import json
import subprocess

from runtime import docker_env, sanitized
from ui_upstream import STOP

BASE = "http://127.0.0.1:8198"
MODEL = "fixture-chat"
MARKER = "kbd-ui-stop-image"


def provider_env():
    value = {"id": "fixture", "baseURL": BASE + "/v1", "model": MODEL}
    return "PROXY_OPENCODE_PROVIDER=" + json.dumps(value, separators=(",", ":"))


# Count closure on the receiving TCP socket. Keep the response open until the
# caller disconnects. Expiration explicitly disqualifies the stream from proof.
PEER = r"""
import http from "node:http";
const fixture = JSON.parse(process.env.KBD_RECEIVER_CONFIG);
const stats = {started: 0, active: 0, disconnected: 0, expired: 0, rejected: 0};
const server = http.createServer(async (req, res) => {
  if (req.method === "GET" && ["/stats", "/after-stop"].includes(req.url)) {
    if (req.url === "/after-stop") {
      const deadline = performance.now() + 4500;
      while (stats.active && performance.now() < deadline) await Bun.sleep(50);
    }
    res.writeHead(200, {"content-type":"application/json"});
    return res.end(JSON.stringify(stats));
  }
  let data;
  try {
    if (req.url !== "/v1/chat/completions" || req.method !== "POST" || req.headers.authorization)
      throw new Error();
    let size = 0;
    const chunks = [];
    for await (const chunk of req) {
      size += chunk.length;
      if (size > 1024 * 1024) throw new Error();
      chunks.push(chunk);
    }
    data = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    const last = data.messages.findLast(m => m.role === "user");
    if (data.stream !== true || data.model !== fixture.model ||
        !last || !JSON.stringify(last.content).includes(fixture.marker)) throw new Error();
  } catch {
    ++stats.rejected;
    res.writeHead(400);
    return res.end();
  }
  function send(delta) {
    const value = {id:"chatcmpl-fixture",object:"chat.completion.chunk",created:1,model:fixture.model,
                   choices:[{index:0,delta,finish_reason:null}]};
    res.write("data: " + JSON.stringify(value) + "\n\n");
  }
  ++stats.started;
  ++stats.active;
  let timedOut = false;
  const socket = req.socket;
  const interval = setInterval(() => send({content:" ."}), 120);
  const expiry = setTimeout(() => {
    timedOut = true;
    ++stats.expired;
    clearInterval(interval);
    socket.destroy();
  }, 90000);
  socket.once("close", () => {
    clearInterval(interval);
    clearTimeout(expiry);
    --stats.active;
    if (!timedOut) ++stats.disconnected;
  });
  res.on("error", () => {});
  res.writeHead(200, {"content-type":"text/event-stream", "cache-control":"no-cache"});
  send({role:"assistant"});
  send({content:fixture.stopText});
});
server.listen(8198, "127.0.0.1");
"""


def start(h, docker):
    cid = h.state["container"]
    configuration = json.dumps({"model": MODEL, "marker": MARKER, "stopText": STOP})
    out = docker("exec", "-d", "-e", "KBD_RECEIVER_CONFIG=" + configuration, cid,
                 "/usr/local/bin/bun", "-e", PEER)
    assert out.returncode == 0, "could not start in-container model recipient"
    result = docker("exec", cid, "curl", "--fail", "--silent", "--retry", "6", "--retry-connrefused",
                    "--retry-delay", "1", "--max-time", "2", BASE + "/stats")
    assert result.returncode == 0, "in-container model recipient did not become ready"
    assert json.loads(result.stdout) == dict(started=0, active=0, disconnected=0, expired=0, rejected=0)
    return {"address": BASE, "model": MODEL, "network": "none"}


def observations(h, after_stop=False):
    path = "/after-stop" if after_stop else "/stats"
    h.count()
    result = subprocess.run(["docker", "exec", h.state["container"], "curl", "--fail", "--silent",
                             "--max-time", "6", BASE + path],
                            env=docker_env(sanitized(h.root / "docker")), capture_output=True,
                            text=True, timeout=8)
    assert result.returncode == 0, "could not observe in-container model recipient"
    state = json.loads(result.stdout)
    assert set(state) == {"started", "active", "disconnected", "expired", "rejected"}
    assert all(type(n) is int and n >= 0 for n in state.values())
    return state
