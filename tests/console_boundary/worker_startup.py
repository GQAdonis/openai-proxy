"""Exercise slow worker startup and cancellation through the installed console API."""
import json
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

from auth_fixture import digest, wait_for
from runtime import directory, origin, port, sanitized, wait_http


WORKER = """\
import json
import pathlib
import sys
import time

pathlib.Path("fixture-worker-started").touch()
time.sleep(13)
driver = sys.argv[1]
print(json.dumps({"v": 1, "type": "ready", "driver": driver, "version": "2.3.0"}), flush=True)
line = sys.stdin.readline()
if line:
    assert json.loads(line)["type"] == "start"
    pathlib.Path("fixture-prompt-dispatched").touch()
    print(json.dumps({"v": 1, "type": "text", "text": "fixture reply"}), flush=True)
    print(json.dumps({"v": 1, "type": "terminal", "status": "completed"}), flush=True)
"""


def verify(binary, root):
    env = sanitized(root)
    workspace = directory(root / "workspace")
    state = directory(root / "state")
    worker = root / "peer.py"
    worker.write_text(WORKER)
    package = directory(root / "node_modules/@opencode/sdk") / "package.json"
    package.write_text(json.dumps({"name": "@opencode/sdk", "version": "2.3.0"}))
    number = port()
    base = origin(number)
    token = "synthetic-local-console-access-token"
    env.update(
        PROXY_CONSOLE_WORKSPACE=str(workspace),
        PROXY_CONSOLE_DATA=str(state),
        PROXY_CONSOLE_TOKEN=token,
        PROXY_SDK_WORKER=str(worker),
        PROXY_SDK_BUN=sys.executable,
        PROXY_OPENCODE_PROVIDER=json.dumps({
            "id": "fixture", "baseURL": "http://127.0.0.1:9/v1", "model": "fixture-model",
        }),
    )
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def api(path, body=None, expected=200):
        request = urllib.request.Request(base + "/api/v1" + path, headers={
            "Authorization": f"Bearer {token}", "Origin": base, "Content-Type": "application/json",
        }, data=json.dumps(body).encode() if body is not None else None)
        with client.open(request, timeout=5) as response:
            assert response.status == expected
            return json.load(response)

    def start():
        conversation = api("/conversations", {"driver": "opencode", "model": "fixture-model"}, 201)
        parent = f"/conversations/{conversation['id']}/turns"
        turn = api(parent, {"prompt": "fixture", "idempotencyKey": "one"}, 202)
        worker_root = state / "workers" / digest("local") / conversation["id"]
        return f"{parent}/{turn['id']}", worker_root

    def completed(turn):
        saved = api(turn)
        return saved if saved["state"] in ("completed", "cancelled", "failed", "indeterminate") else None

    with subprocess.Popen(
        [str(binary), "serve", "--host", "127.0.0.1", "--port", str(number)],
        cwd=workspace, env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ) as proxy:
        try:
            wait_http(base, proxy)
            slow, _ = start()
            result = wait_for(lambda: completed(slow), 30)
            assert result["state"] == "completed" and result["text"] == "fixture reply", result["state"]

            stopped, worker_root = start()
            wait_for(lambda: (worker_root / "fixture-worker-started").exists())
            api(stopped + "/cancel", {})
            result = wait_for(lambda: completed(stopped), 5)
            assert result["state"] == "cancelled", result["state"]
            assert not (worker_root / "fixture-prompt-dispatched").exists()
        finally:
            proxy.terminate()
            proxy.wait(timeout=10)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as directory_name:
        verify(Path(sys.argv[1]).resolve(strict=True), Path(directory_name))
    print("Delayed OpenCode readiness and cancellation before dispatch passed")
