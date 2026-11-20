"""Read and retain independent observations only in the disposable B4 profile."""
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import uuid


class MissingPeer(FileNotFoundError):
    pass


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def case(h, label, observation):
    def checked():
        try:
            return observation()
        except (AssertionError, MissingPeer, TimeoutError):
            raise
        except Exception as exc:
            # Avoid echoing a client's raw payload or request headers into receipts.
            raise AssertionError(type(exc).__name__) from None
    return h.check(label, checked)


def note(h, label, value):
    h.note(label + " " + uuid.uuid4().hex[:12], value)


def private(h, path):
    path = Path(path).resolve()
    require(path.is_relative_to(h.root.resolve()), "Target outside disposable run root")
    return path


def snapshot(h, path, label):
    path = private(h, path)
    if not path.exists():
        return {"exists": False}
    raw = path.read_bytes()
    receipt = h.root / "admin-originals" / (uuid.uuid4().hex + ".bin")
    receipt.parent.mkdir(mode=0o700, exist_ok=True)
    with receipt.open("xb") as output:
        os.fchmod(output.fileno(), 0o600)
        output.write(raw)
    evidence = {"exists": True, "sha256": hashlib.sha256(raw).hexdigest(),
                "bytes": len(raw), "mode": stat.S_IMODE(path.stat().st_mode)}
    note(h, label, {**evidence, "original": str(receipt.relative_to(h.root))})
    return evidence


@contextlib.contextmanager
def seed(h, path, raw):
    path = private(h, path)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.exists():
        raise MissingPeer("A clean dedicated configuration target is required")
    with path.open("xb") as output:
        os.fchmod(output.fileno(), 0o640)
        output.write(raw)
    try:
        yield
    finally:
        path.unlink(missing_ok=True)


def http(h, path, method="GET", body=None, actor="local", statuses=(200,), **options):
    code, headers, raw = h.http(path, method=method, body=body, actor=actor, **options)
    require(code in statuses, f"{method} {path.split('?')[0]} returned HTTP {code}")
    require(all(value.encode() not in raw for value in h.keys.values() if isinstance(value, str) and value),
            "Endpoint disclosed a synthetic authorization token")
    return json.loads(raw) if raw and "json" in headers.get("content-type", "") else raw


def command(h, argv, cwd, env=None):
    env = dict(h.env if env is None else env)
    for name in ("HOME", "CODEX_HOME"):
        private(h, env[name])
    # PATH names only installed clients. Use a fresh working directory and forbid
    # automatic updates and remotely refreshed model catalogues where supported.
    env.update({"DISABLE_AUTOUPDATER": "1", "OPENCODE_DISABLE_AUTOUPDATE": "1",
                "OPENCODE_DISABLE_MODELS_FETCH": "1", "NO_COLOR": "1"})
    argv = [str(v) for v in argv]
    child = subprocess.Popen(argv, cwd=private(h,cwd), env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        out, err = child.communicate(timeout=35)
    finally:
        # A CLI wrapper may have left a subprocess attached to its private group.
        try:
            os.killpg(child.pid,signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=3)
    result = subprocess.CompletedProcess(argv,child.returncode,out,err)
    logs = {}
    for stream, data in (("stdout",out),("stderr",err)):
        # Store only output from these synthetic profiles. A credential-bearing
        # result can be identified by digest, never copied into the evidence.
        capture = h.root / "admin-originals" / (uuid.uuid4().hex + "." + stream)
        capture.parent.mkdir(mode=0o700, exist_ok=True)
        redacted = data
        keys = [*h.keys.values(),env.get("PROXY_MCP_TOKEN"),env.get("ANTHROPIC_API_KEY")]
        auth = private(h, Path(h.env["CODEX_HOME"]) / "auth.json")
        if auth.is_file():
            record=json.loads(auth.read_bytes())
            keys.extend((record.get("tokens") or {}).values())
            keys.append(record.get("OPENAI_API_KEY"))
        for secret in keys:
            if isinstance(secret,str) and secret:
                redacted=redacted.replace(secret.encode(),b"[redacted]")
        redacted=re.sub(rb"(?i)Bearer [A-Za-z0-9._=/-]+",b"Bearer [redacted]",redacted)
        with capture.open("xb") as output:
            os.fchmod(output.fileno(),0o600)
            output.write(redacted)
        logs[stream]={"sha256":hashlib.sha256(data).hexdigest(),"output":str(capture.relative_to(h.root)),
                      "redacted":redacted!=data}
    note(h, "client process", {"argv":argv,"exit":result.returncode,**logs})
    return result


def client(h, name, cwd, expected=None):
    binary = shutil.which(name, path=h.env["PATH"])
    if not binary:
        raise MissingPeer(f"{name} is absent from the isolated client PATH")
    result = command(h, [binary, "--version"], cwd)
    bundle_cli = Path("/Applications/Zed.app/Contents/MacOS/cli")
    if (name == "zed" and sys.platform == "darwin" and bundle_cli.is_file()
            and (result.returncode or not result.stdout.lstrip().startswith(b"Zed "))):
        binary = str(bundle_cli)
        result = command(h, [binary, "--version"], cwd)
    version = re.search(rb"(?<![.\d])v?(\d+\.\d+\.\d+(?:-[\w.-]+)?)(?![.\d])", result.stdout)
    if not version or result.returncode:
        raise MissingPeer(f"{name} version unavailable from the isolated profile")
    version = version.group(1).decode()
    h.note(name + " version", version)
    if expected and version != expected:
        raise MissingPeer(f"{name} {version} installed, contract requires {expected}")
    return binary


def outputs(result):
    values = []
    raw = result.stdout.decode().strip()
    while raw:
        item, end = json.JSONDecoder().raw_decode(raw)
        values.append(item)
        raw = raw[end:].lstrip()
    return values
