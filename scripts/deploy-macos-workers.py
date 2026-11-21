#!/usr/bin/env python3
"""Deploy a worker update or a full release to the existing macOS LaunchAgent."""

import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
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
BUILD_INPUTS = ("Cargo.toml", "Cargo.lock", "build.rs", "src", "data", "frontend", "workers", "plugin")


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


def bind_launcher(active_worker, worker, launcher):
    old_launcher_source = active_worker / "codex-exec.c"
    if (launcher is None and old_launcher_source.exists()
            and old_launcher_source.read_bytes() != (worker / "codex-exec.c").read_bytes()):
        raise RuntimeError("The launcher source changed. Supply the new native executable with --launcher")
    stage_launcher(launcher or active_worker / "codex-exec", worker / "codex-exec")


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
    (worker / "node_modules").symlink_to((active_worker / "node_modules").resolve(strict=True))
    (release / "plugin").symlink_to((active_worker.parent / "plugin").resolve(strict=True))
    bind_launcher(active_worker, worker, launcher)
    return worker / "main.mjs"


def version(executable):
    return subprocess.check_output([str(executable), "--version"], text=True, timeout=20).strip()


def stage_full(revision, release, active_worker, launcher, bun):
    with tempfile.TemporaryDirectory(prefix="knowme-build-") as temporary:
        build = Path(temporary)
        with (build / "source.tar").open("wb") as archive:
            subprocess.run(["git", "archive", revision, *BUILD_INPUTS], cwd=ROOT, stdout=archive, check=True)
        subprocess.run(["/usr/bin/tar", "-xf", "source.tar"], cwd=build, check=True)

        worker = release / "workers"
        plugin = release / "plugin"
        shutil.copytree(build / "workers", worker, ignore=shutil.ignore_patterns("codex-exec"))
        shutil.copytree(build / "plugin", plugin)
        bind_launcher(active_worker, worker, launcher)

        frontend = build / "frontend"
        engines = json.loads((frontend / "package.json").read_bytes())["engines"]
        if version("node") != "v" + engines["node"] or version("npm") != engines["npm"]:
            raise RuntimeError(f"Full build requires Node {engines['node']} and npm {engines['npm']}")
        # Keep stdout reserved for the JSON deployment receipt.
        for package in worker, plugin:
            subprocess.run([str(bun), "install", "--frozen-lockfile", "--production"],
                           cwd=package, stdout=sys.stderr, check=True)
        subprocess.run([str(bun), "run", "build"], cwd=plugin, stdout=sys.stderr, check=True)
        candidates = sorted((worker / "node_modules" / "@openai").glob("codex-*/vendor/*/bin/codex"))
        sdk_version = json.loads((worker / "package.json").read_bytes())["dependencies"]["@openai/codex-sdk"]
        if len(candidates) != 1 or version(candidates[0]) != f"codex-cli {sdk_version}":
            raise RuntimeError("The full release must contain the locked platform Codex CLI")

        subprocess.run(["npm", "ci", "--include=dev", "--no-audit", "--no-fund"],
                       cwd=frontend, stdout=sys.stderr, check=True)
        target = build / "target"
        subprocess.run(["cargo", "build", "--release", "--locked", "--bin", "openai-proxy",
                        "--target-dir", str(target)], cwd=build, stdout=sys.stderr, check=True)
        binary = release / "openai-proxy"
        shutil.copy2(target / "release" / "openai-proxy", binary)
        if not version(binary).startswith("openai-proxy "):
            raise RuntimeError("The staged proxy binary failed its startup probe")
    return worker / "main.mjs", binary, candidates[0]


def publish(source, destination):
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as file:
        temporary = Path(file.name)
    try:
        shutil.copy2(source, temporary)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def publish_executable(source, destination, *, symlink):
    with tempfile.TemporaryDirectory(dir=destination.parent) as folder:
        staged = Path(folder) / destination.name
        if symlink:
            staged.symlink_to(source.resolve(strict=True))
        else:
            os.link(source, staged)
        staged.replace(destination)


def digest(path):
    with path.open("rb") as artifact:
        return hashlib.file_digest(artifact, "sha256").hexdigest()


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


def deploy(plist, config, release, launcher, revision, port, *, full=False):
    environment = config["EnvironmentVariables"]
    active_worker = Path(environment["PROXY_SDK_WORKER"]).resolve(strict=True).parent
    try:
        if full:
            installed_binary = Path(config["ProgramArguments"][0])
            expected_binary = Path.home() / ".local/bin/openai-proxy"
            if installed_binary != expected_binary or not installed_binary.is_file():
                raise RuntimeError("Expected the installed KnowMe executable at ~/.local/bin/openai-proxy")
            bun = Path(environment["PROXY_SDK_BUN"]).resolve(strict=True)
            worker, binary, codex = stage_full(revision, release, active_worker, launcher, bun)
            environment["PROXY_CODEX_BINARY"] = str(codex)
            candidate_digest = digest(binary)
        else:
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
    if full:
        saved_binary = backup / installed_binary.name
        was_symlink = installed_binary.is_symlink()
        if was_symlink:
            saved_binary.symlink_to(installed_binary.resolve(strict=True))
        else:
            os.link(installed_binary, saved_binary)
    candidate = backup / "candidate.plist"
    candidate.write_bytes(plistlib.dumps(config))
    subprocess.run(["/usr/bin/plutil", "-lint", str(candidate)], check=True, capture_output=True)

    domain = f"gui/{os.getuid()}"
    target = f"{domain}/{LABEL}"
    replacing_plist = False
    try:
        launch("bootout", target)
        replacing_plist = True
        if full:
            publish_executable(binary, installed_binary, symlink=True)
        publish(candidate, plist)
        boot(plist, domain, port)
    except BaseException:
        try:
            if replacing_plist:
                if full:
                    publish_executable(saved_binary, installed_binary, symlink=was_symlink)
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
    receipt = {"commit": revision, "worker": str(worker), "rollbackPlist": str(saved), "health": 200}
    if full:
        receipt.update(binary=str(binary), binarySha256=candidate_digest, rollbackBinary=str(saved_binary),
                       previousBinarySha256=digest(saved_binary), codex=str(codex))
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full", action="store_true", help="build and deploy the proxy, console, and locked SDK runtime")
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
        kind = "full" if args.full else "workers"
        release = Path(tempfile.mkdtemp(prefix=f"{kind}-{revision[:12]}-", dir=releases))
        print(json.dumps(deploy(plist, config, release, args.launcher, revision, port, full=args.full)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        sys.exit(str(error))
