"""Observe atomic credential replacement from the packaged service on real bind mounts."""

import json
import os
import re
import secrets
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from auth_fixture import credential, digest, wait_for
import runtime


# Only the model recipient sees the outbound bearer. Its streaming reply contains
# the account and a digest, allowing the normal HTTP client to verify delivery.
RECIPIENT = r"""
import { createHash } from "node:crypto";
let requests = 0;
Bun.serve({
  hostname: "127.0.0.1", port: 8199,
  async fetch(req) {
    const path = new URL(req.url).pathname;
    if (req.method === "GET" && path === "/count") return Response.json(requests);
    ++requests;
    if (req.method !== "POST" || path !== "/responses") return new Response(null, {status: 404});
    await req.arrayBuffer();
    const owner = req.headers.get("chatgpt-account-id");
    const auth = req.headers.get("authorization") ?? "";
    if (!["owner-a", "owner-b"].includes(owner) || !auth.startsWith("Bearer "))
      return new Response(null, {status: 403});
    const marker = owner + "#" + createHash("sha256").update(auth.slice(7)).digest("hex");
    const events = [
      {type:"response.output_text.delta", output_index:0, content_index:0, delta:marker},
      {type:"response.completed", response:{id:"fixture", model:"gpt-5.5", status:"completed"}}
    ];
    return new Response(events.map(e => "data: " + JSON.stringify(e) + "\n\n").join(""),
                        {headers: {"Content-Type": "text/event-stream"}});
  }
});
"""

CHAT = {"model": "gpt-5.5", "messages": [{"role": "user", "content": "echo fixture"}], "stream": False}
STORE = "/tmp/kbd-credential-home"
RECIPIENT_COUNT = "http://127.0.0.1:8199/count"


def publish(store, doc, uid, gid):
    # Match the process UID without widening directory or bearer permissions.
    with tempfile.NamedTemporaryFile(dir=store.parent, mode="w", delete=False) as stream:
        if os.getuid() == 0:
            os.fchown(stream.fileno(), uid, gid)
        json.dump(doc, stream)
    started = time.monotonic()
    Path(stream.name).replace(store)
    return started


def mounted_store(h, negative):
    if not h.state.get("image"):
        h.block("credential bind mount", "frozen SDK image unavailable")
        return
    label = "single-file mount refuses credential delivery" if negative else "directory mount follows external login"
    h.check(label, lambda: exercise(h, negative))


def exercise(h, single):
    with tempfile.TemporaryDirectory(prefix="auth-mount-", dir=h.root) as folder:
        case = Path(folder)
        home = runtime.directory(case / "codex")
        uid, gid = (os.getuid(), os.getgid()) if os.getuid() else (65534, 65534)
        if os.getuid() == 0:
            os.chown(home, uid, gid)
        store = home / "auth.json"
        source = store if single else home
        target = STORE + "/auth.json" if single else STORE
        if single:
            publish(store, credential("owner-a"), uid, gid)
        label = "kbd.auth-mount=" + secrets.token_hex(16)
        mount = f"type=bind,src={source},dst={target}" + (",readonly" if single else "")
        env = runtime.docker_env(runtime.sanitized(case / "docker"))
        env["PROXY_CONSOLE_TOKEN"] = h.keys["image"]

        def docker(*args, count=True):
            if count:
                h.count()
            result = subprocess.run(["docker", *args], env=env, capture_output=True, text=True, timeout=25)
            assert result.returncode == 0, f"credential mount docker {args[0]} exited {result.returncode}"
            return result.stdout.strip()

        def inspect():
            fmt = '{{.Image}} {{.State.Pid}} {{.State.StartedAt}} {{.HostConfig.NetworkMode}}\n{{json .Mounts}}'
            process, mounts = docker("inspect", "--format", fmt, cid).splitlines()
            image, pid, started, network = process.split()
            assert image == h.state["image"] and int(pid) > 0 and network == "none"
            matches = [m for m in json.loads(mounts) if m["Type"] == "bind" and m["Destination"] == target]
            assert len(matches) == 1 and matches[0]["RW"] == (not single), "unexpected credential mount"
            return pid, started

        def request(path, method="GET", body=None):
            code, _, raw = h.http(path, method, body, actor="image", base=base)
            return code, json.loads(raw)

        def status(reload=False):
            path = "/api/v1/me/connection"
            code, data = request(path + "/reload", "POST", {}) if reload else request(path)
            assert code == 200, f"credential mount connection HTTP {code}"
            return data

        relay = None
        try:
            cid = docker("run", "-d", "--pull=never", "--network", "none", "--label", label,
                         "--user", f"{uid}:{gid}", "--workdir", "/tmp", "--mount", mount, "--entrypoint", "/bin/sh",
                         "-e", "PROXY_CONSOLE_TOKEN", "-e", "HOME=/tmp/kbd-home", "-e", "CODEX_HOME=" + STORE,
                         "-e", "PROXY_CONSOLE_DATA=/tmp/kbd-state", "-e", "PROXY_CONSOLE_WORKSPACE=/tmp/kbd-work",
                         "-e", "PROXY_SDK_WORKER=/tmp/kbd-no-chat-sdk",
                         "-e", "PROXY_BACKEND_CREDENTIAL_CLASS=subscription",
                         h.state["image"], "-ec",
                         'umask 077; mkdir -p "$HOME" /tmp/kbd-work; exec openai-proxy serve '
                         '--host 127.0.0.1 --port 8181 --backend-url http://127.0.0.1:8199/responses')
            assert re.fullmatch("[0-9a-f]{64}", cid), "container creation did not yield a full ID"
            initial_process = inspect()
            access_test = "-r" if single else "-w"
            assert docker("exec", cid, "sh", "-ec", 'test "$1" "$2"; id -u', "sh",
                          access_test, target) == str(uid), "suite-owned bind mount unavailable to the process UID"
            docker("exec", "-d", cid, "/usr/local/bin/bun", "-e", RECIPIENT)
            counter_args = ("exec", cid, "curl", "-fsS", "--retry", "8", "--retry-connrefused",
                            "--retry-delay", "1", "--max-time", "2", RECIPIENT_COUNT)
            assert docker(*counter_args) == "0", "isolated model recipient did not start with zero calls"
            port = runtime.port()
            base = runtime.origin(port)
            relay = runtime.spawn(h, [sys.executable, "-B", str(h.source / "tests/console_boundary/runtime.py"),
                                      str(port), cid], env, "auth-mount-loopback", case)
            runtime.wait_http(base, relay)
            if single:
                def rejected(state):
                    assert state["status"] == "unsupported_store" and state["source"] == "single_file_mount"
                    assert not state["capabilities"]["refresh"]
                    code, data = request("/v1/chat/completions", "POST", CHAT)
                    assert code >= 400 and data["error"]["type"] == "auth_unavailable", f"unexpected chat HTTP {code}"
                before = status()
                rejected(before)
                old_inode = store.stat().st_ino
                publish(store, credential("owner-b"), uid, gid)
                assert store.stat().st_ino != old_inode, "host file was not replaced"
                after = status(reload=True)
                rejected(after)
                assert after["generation"] == before["generation"]
                assert docker(*counter_args) == "0", "unsupported single-file mount caused egress"
                outcome = {"status": after["status"], "source": after["source"], "recipientCalls": 0}
            else:
                before = status()
                assert before["status"] == "signed_out"
                generation = before["generation"]
                deliveries = []
                for owner in ("owner-a", "owner-a", "owner-b"):
                    doc = credential(owner)
                    fingerprint = digest(doc["tokens"]["access_token"])
                    expected = owner + "#" + fingerprint
                    started = publish(store, doc, uid, gid)

                    def delivered():
                        code, data = request("/v1/chat/completions", "POST", CHAT)
                        return code == 200 and data["choices"][0]["message"]["content"] == expected

                    wait_for(delivered, 2)
                    elapsed = time.monotonic() - started
                    assert elapsed <= 2, "atomic replacement missed two-second recipient deadline"
                    state = status()
                    assert state["status"] == "ready" and state["source"] == "file"
                    assert state["generation"] > generation
                    generation = state["generation"]
                    deliveries.append({"owner": owner, "bearerSha256": fingerprint, "generation": generation,
                                       "elapsedMs": round(elapsed * 1000)})
                effects = int(docker(*counter_args))
                assert effects >= len(deliveries)
                outcome = {"deliveries": deliveries, "recipientCalls": effects}
            assert inspect() == initial_process, "service process changed during mount observation"
            return dict(outcome, image=h.state["image"], pid=initial_process[0], startedAt=initial_process[1],
                        network="none", mountTarget=target, readOnly=single)
        finally:
            try:
                if relay is not None and relay.poll() is None:
                    relay.terminate()
                    try:
                        relay.wait(timeout=4)
                    except subprocess.TimeoutExpired:
                        relay.kill()
                        relay.wait(timeout=4)
            finally:
                # Discover by unique case label even if create returned no usable ID.
                ids = docker("ps", "-aq", "--no-trunc", "--filter", "label=" + label, count=False).split()
                if ids:
                    assert all(re.fullmatch("[0-9a-f]{64}", i) for i in ids)
                    docker("rm", "-f", *ids, count=False)
