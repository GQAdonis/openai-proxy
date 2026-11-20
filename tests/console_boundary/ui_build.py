"""Two isolated derivative builds for the packaged web asset controls."""
import hashlib
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
import urllib.request

IGNORE = {".git", "target", "node_modules", "dist", "__pycache__"}
MARKER = b"fixture-kbd-rebuild-marker"
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def digest_file(path):
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def tree_id(root):
    h = hashlib.sha256()
    for here, dirs, names in os.walk(root):
        dirs[:] = sorted(d for d in dirs if d not in IGNORE)
        for name in sorted(names):
            path = Path(here, name)
            if path.is_symlink() or path.is_file():
                h.update(str(path.relative_to(root)).encode() + b"\0")
                h.update((os.readlink(path) if path.is_symlink() else digest_file(path)).encode() + b"\0")
    return h.hexdigest()


def served_html(binary, directory, env):
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    req = urllib.request.Request(f"http://127.0.0.1:{port}/ui/", headers={"Accept": "text/html"})
    with (directory / "server.log").open("ab") as log:
        service = subprocess.Popen([binary, "serve", "--host", "127.0.0.1", "--port", str(port)],
                                   cwd=directory, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and service.poll() is None:
                try:
                    with OPENER.open(req, timeout=1) as response:
                        return response.read()
                except OSError:
                    time.sleep(0.15)
            raise AssertionError("derivative service did not serve its embedded index")
        finally:
            if service.poll() is None:
                service.terminate()
                try:
                    service.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    service.kill()
                    service.wait()


def derivative(h, broken):
    frozen = Path(h.source)
    original = tree_id(frozen)
    installed = digest_file(h.binary)
    kind = "failed" if broken else "incremental"
    work = h.root / ("ui-build-" + kind)
    work.mkdir(mode=0o700)
    src = work / "source"
    shutil.copytree(frozen, src, symlinks=True, ignore=shutil.ignore_patterns(*IGNORE))
    # Copy the already locked dependency install, retaining npm's relative bin links.
    shutil.copytree(frozen / "frontend/node_modules", src / "frontend/node_modules", symlinks=True)
    assert tree_id(src) == original, "disposable source differs from the frozen source"
    runtime = work / "empty"
    runtime.mkdir()
    from runtime import sanitized
    env = dict(sanitized(work / "profile"), CARGO_TARGET_DIR=str(work / "target"), CARGO_HOME=str(h.root / "cargo"),
               RUSTUP_HOME=os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
               CODEX_HOME=str(work / "codex"), PROXY_CONSOLE_DATA=str(work / "state"))
    binary = work / "target/release/openai-proxy"

    def build(stage):
        path = h.evidence / ("ui-derivative-" + kind + "-" + stage + ".log")
        with path.open("wb") as log:
            result = subprocess.run(["cargo", "build", "--release", "--locked", "--offline"],
                                    cwd=src, env=env, stdin=subprocess.DEVNULL,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=2400)
        return result.returncode, path

    try:
        rc, _ = build("baseline")
        assert rc == 0 and binary.is_file(), f"derivative baseline failed to build; see ui-derivative-{kind}-baseline.log"
        before = served_html(binary, runtime, env)
        assert MARKER not in before and b'type="module"' in before, "invalid derivative baseline"
        before_bin = digest_file(binary)
        if broken:
            with (src / "frontend/src/main.tsx").open("ab") as out:
                out.write(b"\nconst invalidKbdControl: = ;\n")
        else:
            html = src / "frontend/index.html"
            original_html = html.read_bytes()
            assert b"</head>" in original_html
            html.write_bytes(original_html.replace(b"</head>", b'<meta name="' + MARKER + b'" content="1"></head>'))
        candidate = tree_id(src)
        assert candidate != original
        rc, log = build("mutated")
        if broken:
            assert rc != 0 and b"invalidKbdControl" in (src / "frontend/src/main.tsx").read_bytes()
            content = log.read_bytes().lower()
            assert b"main.tsx" in content and b"error ts" in content, "build failed outside the induced frontend error"
        else:
            assert rc == 0, "incremental Cargo build failed"
            after = served_html(binary, runtime, env)
            assert MARKER in after and before != after and digest_file(binary) != before_bin
        return {"originalSource": original, "derivativeSource": candidate,
                "originalHtml": hashlib.sha256(before).hexdigest(), "baselineBinary": before_bin,
                "resultBinary": digest_file(binary) if binary.exists() else None,
                "resultHtml": None if broken else hashlib.sha256(after).hexdigest(),
                "resultExit": rc, "log": log.name}
    finally:
        assert tree_id(frozen) == original and digest_file(h.binary) == installed, "frozen inputs changed"


def check(h, broken):
    label = "controlled failed frontend build" if broken else "incremental embedded asset replacement"
    if not (Path(h.source) / "frontend/node_modules").is_dir() or not shutil.which("cargo", path=h.env["PATH"]):
        h.block(label, "frozen locked frontend install or Cargo unavailable")
    else:
        h.check(label, lambda: derivative(h, broken))


def degraded(h):
    from runtime import directory, origin, port, sanitized, wait_http
    from ui_browser import browser
    work = directory(h.root / "ui-web-only")
    cwd = directory(work / "empty")
    bare = work / "bin/openai-proxy"
    directory(bare.parent)
    shutil.copy2(h.binary, bare)
    assert digest_file(bare) == digest_file(h.binary), "standalone binary is not the installed release"
    env = sanitized(directory(work / "profile"))
    state = directory(work / "state")
    number = port()
    base = origin(number)
    env.update(PROXY_CONSOLE_TOKEN=h.keys["local"], PROXY_CONSOLE_DATA=str(state),
               PROXY_CONSOLE_ORIGIN=base, PROXY_CONSOLE_WORKSPACE=str(cwd),
               HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1", NO_PROXY="127.0.0.1")
    with (work / "service.log").open("wb") as log:
        child = subprocess.Popen([str(bare), "serve", "--host", "127.0.0.1", "--port", str(number)],
                                 cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            wait_http(base, child)
            with browser(h, base, "web-only", state / "pairing-token") as page:
                details = page.request.get(base + "/api/v1/me/connection").json()
                assert {d["id"] for d in details["drivers"]} == {"opencode", "codex"}
                assert all(d["available"] is False for d in details["drivers"]), "web-only service advertised a native driver"
                driver = page.get_by_label("Driver", exact=True)
                for d in details["drivers"]:
                    assert driver.locator(f'option[value="{d["id"]}"]').is_disabled()
                page.get_by_label("Message", exact=True).fill("kbd-web-only")
                assert page.get_by_role("button", name="Send message").is_disabled(), "web-only browser offers an unusable send"
                return {"binary": digest_file(bare), "disabledDrivers": [d["id"] for d in details["drivers"]]}
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
