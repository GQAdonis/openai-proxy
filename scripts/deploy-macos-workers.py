#!/usr/bin/env python3
"""Deploy the committed native workers to an existing local macOS LaunchAgent."""

import argparse
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
LABEL = "dev.prometheusags.openai-proxy"
WORKER_FILES = ("main.mjs", "codex.mjs", "codex-metrics.mjs", "opencode.mjs", "package.json", "bun.lock", "codex-exec.c")


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def stage_launcher(source, destination):
    source = source.resolve(strict=True)
    if not source.is_file() or not os.access(source, os.X_OK):
        raise RuntimeError("The native Codex launcher must be an executable file")
    # A byte-for-byte copy of the deployed Mach-O previously hung in dyld.
    # Preserve the tested file's inode. An incompatible filesystem must fail
    # before launchd or its plist is touched.
    os.link(source, destination)
    try:
        probe = subprocess.run(
            [str(destination), "exec"],
            env={"PATH": "/usr/bin:/bin", "PROXY_CODEX_BINARY": "/usr/bin/true"},
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            cwd=destination.parent, timeout=8,
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError("The staged Codex launcher timed out before launch") from None
    if probe.returncode != 0:
        raise RuntimeError(f"The staged Codex launcher failed its startup probe ({probe.returncode})")


def stage_workers(revision, release, active_worker, launcher):
    worker = release / "workers"
    worker.mkdir()
    for name in WORKER_FILES:
        (worker / name).write_bytes(git("show", f"{revision}:workers/{name}"))
    old_package = json.loads((active_worker / "package.json").read_bytes())
    new_package = json.loads((worker / "package.json").read_bytes())
    old_lock = active_worker / "bun.lock"
    if (old_package.get("dependencies") != new_package.get("dependencies")
            or (old_lock.exists() and old_lock.read_bytes() != (worker / "bun.lock").read_bytes())):
        raise RuntimeError("Worker dependencies changed. Prepare the updated runtime before deploying")
    old_launcher_source = active_worker / "codex-exec.c"
    if (launcher is None and old_launcher_source.exists()
            and old_launcher_source.read_bytes() != (worker / "codex-exec.c").read_bytes()):
        raise RuntimeError("The launcher source changed. Supply the new native executable with --launcher")
    (worker / "node_modules").symlink_to((active_worker / "node_modules").resolve(strict=True))
    (release / "plugin").symlink_to((active_worker.parent / "plugin").resolve(strict=True))
    stage_launcher(launcher or active_worker / "codex-exec", worker / "codex-exec")
    return worker / "main.mjs"


def publish(source, destination):
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as file:
        temporary = Path(file.name)
    try:
        shutil.copy2(source, temporary)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def launch(*args, check=True):
    return subprocess.run(["/bin/launchctl", *args], capture_output=True, check=check, timeout=20)


def boot(plist, domain, port):
    launch("bootstrap", domain, str(plist))
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            with client.open(f"http://127.0.0.1:{port}/health", timeout=1.5) as response:
                if response.status == 200:
                    return
        except OSError:
            time.sleep(0.25)
    raise RuntimeError("KnowMe LaunchAgent did not become healthy")


def deploy(plist, config, release, launcher, revision, port):
    environment = config["EnvironmentVariables"]
    active_worker = Path(environment["PROXY_SDK_WORKER"]).resolve(strict=True).parent
    try:
        worker = stage_workers(revision, release, active_worker, launcher)
    except BaseException:
        shutil.rmtree(release)
        raise
    environment["PROXY_SDK_WORKER"] = str(worker)

    rollback_root = Path.home() / "Library/Application Support/OpenAI Proxy/rollbacks"
    rollback_root.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    backup = Path(tempfile.mkdtemp(prefix=f"{timestamp}-", dir=rollback_root))
    saved = backup / plist.name
    shutil.copy2(plist, saved)
    candidate = backup / "candidate.plist"
    candidate.write_bytes(plistlib.dumps(config))
    subprocess.run(["/usr/bin/plutil", "-lint", str(candidate)], check=True, capture_output=True)

    domain = f"gui/{os.getuid()}"
    target = f"{domain}/{LABEL}"
    replacing_plist = False
    try:
        launch("bootout", target)
        replacing_plist = True
        publish(candidate, plist)
        boot(plist, domain, port)
    except BaseException:
        try:
            if replacing_plist:
                publish(saved, plist)
                launch("bootout", target, check=False)
                boot(plist, domain, port)
            elif launch("print", target, check=False).returncode != 0:
                # A timed-out bootout can still have unloaded the original job.
                boot(plist, domain, port)
        except Exception as rollback_error:
            raise RuntimeError(f"Rollback failed. Saved LaunchAgent plist: {saved}") from rollback_error
        print(f"Previous LaunchAgent configuration is active. Saved plist: {saved}", file=sys.stderr)
        raise
    return {"commit": revision, "worker": str(worker), "rollbackPlist": str(saved), "health": 200}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", type=Path, help="verified native launcher for an intentional launcher upgrade")
    args = parser.parse_args()
    if sys.platform != "darwin" or os.getuid() == 0:
        parser.error("run as the macOS LaunchAgent owner")
    os.umask(0o077)
    plist = Path.home() / "Library/LaunchAgents" / f"{LABEL}.plist"
    config = plistlib.loads(plist.read_bytes())
    active_worker = Path(config["EnvironmentVariables"]["PROXY_SDK_WORKER"]).resolve(strict=True)
    releases = active_worker.parents[2]
    if config["Label"] != LABEL or active_worker.name != "main.mjs" or releases.name != "releases":
        raise RuntimeError("Expected a versioned KnowMe worker installation")
    program = config["ProgramArguments"]
    if program[program.index("--host") + 1] != "127.0.0.1":
        raise RuntimeError("Expected the KnowMe console's IPv4 loopback listener")
    port = int(program[program.index("--port") + 1])
    revision = git("rev-parse", "HEAD").decode().strip()
    with (releases / ".worker-deploy.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if config != plistlib.loads(plist.read_bytes()):
            raise RuntimeError("The active LaunchAgent changed during deployment preparation")
        release = Path(tempfile.mkdtemp(prefix=f"workers-{revision[:12]}-", dir=releases))
        print(json.dumps(deploy(plist, config, release, args.launcher, revision, port)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        sys.exit(str(error))
