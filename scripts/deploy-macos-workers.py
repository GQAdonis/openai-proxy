#!/usr/bin/env python3
"""Deploy or roll back a release of the existing macOS LaunchAgent."""

import argparse
from datetime import datetime, timezone
import fcntl
from functools import partial
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
RECEIPT_NAME = "release-receipt.json"


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
    probe_launcher(destination)


def probe_launcher(executable):
    try:
        probe = subprocess.run(
            [str(executable), "exec"],
            env={"PATH": "/usr/bin:/bin", "PROXY_CODEX_BINARY": "/usr/bin/true"},
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            cwd=executable.parent, timeout=8,
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
    with tempfile.TemporaryDirectory(prefix="build-", dir=release) as temporary:
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
        (target / "release" / "openai-proxy").replace(binary)
        help_text = subprocess.check_output([str(binary), "--help"], text=True, timeout=20)
        if "Usage: openai-proxy " not in help_text:
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


def launcher_identity(worker):
    launcher = worker.with_name("codex-exec")
    return {"inode": launcher.stat().st_ino, "sha256": digest(launcher)}


def rollback_root():
    return Path.home() / "Library/Application Support/OpenAI Proxy/rollbacks"


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
        else:
            worker = stage_workers(revision, release, active_worker, launcher)
    except BaseException:
        shutil.rmtree(release)
        raise
    environment["PROXY_SDK_WORKER"] = str(worker)

    root = rollback_root()
    root.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    backup = Path(tempfile.mkdtemp(prefix=f"{timestamp}-", dir=root))
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

    receipt = {"commit": revision, "worker": str(worker), "rollbackPlist": str(saved)}
    install = None
    restore = None
    if full:
        receipt_file = backup / RECEIPT_NAME
        receipt.update(binary=str(binary), binarySha256=digest(binary), rollbackBinary=str(saved_binary),
                       previousBinarySha256=digest(saved_binary), codex=str(codex), receiptPath=str(receipt_file),
                       plistSha256=digest(candidate), previousPlistSha256=digest(saved),
                       launcherIdentity=launcher_identity(worker),
                       previousLauncherIdentity=launcher_identity(active_worker / "main.mjs"))
        receipt_file.write_text(json.dumps(receipt, indent=2) + "\n")
        install = partial(publish_executable, binary, installed_binary, symlink=True)
        restore = partial(publish_executable, saved_binary, installed_binary, symlink=was_symlink)
    cutover(plist, candidate, saved, port, install, restore)
    receipt["health"] = 200
    return receipt


def cutover(plist, candidate, saved, port, install, restore):
    domain = f"gui/{os.getuid()}"
    target = f"{domain}/{LABEL}"
    replacing_plist = False
    try:
        launch("bootout", target)
        replacing_plist = True
        if install:
            install()
        publish(candidate, plist)
        boot(plist, domain, port)
    except BaseException:
        try:
            if replacing_plist:
                if restore:
                    restore()
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


def rollback(plist, config, releases, receipt_path, port):
    receipt = json.loads(receipt_path.read_bytes())
    if not isinstance(receipt, dict):
        raise ValueError("Expected a full release receipt object")
    receipt = receipt.get("deployment", receipt)
    artifact_keys = ("commit", "worker", "binary", "binarySha256", "rollbackBinary", "previousBinarySha256",
                     "rollbackPlist")
    if not isinstance(receipt, dict) or any(not isinstance(receipt.get(key), str) for key in artifact_keys):
        raise ValueError("Expected a full release receipt with binary and plist backups")

    saved = Path(receipt["rollbackPlist"])
    backup = saved.parent.resolve(strict=True)
    saved_binary = Path(receipt["rollbackBinary"])
    installed = Path.home() / ".local/bin/openai-proxy"
    if (backup.parent != rollback_root().resolve(strict=True) or saved != backup / plist.name
            or saved_binary != backup / installed.name or saved.is_symlink()):
        raise ValueError("The receipt must reference the local LaunchAgent backup pair")

    integrity_keys = ("plistSha256", "previousPlistSha256", "launcherIdentity", "previousLauncherIdentity")
    if not all(key in receipt for key in integrity_keys):
        recorded = backup / RECEIPT_NAME
        if not recorded.is_file():
            raise ValueError("This receipt predates plist and launcher checksums. A verified full release receipt is required")
        verified = json.loads(recorded.read_bytes())
        if not isinstance(verified, dict) or any(verified.get(key) != receipt[key] for key in artifact_keys):
            raise ValueError("The legacy receipt does not match the verified full release receipt")
        receipt = verified
    if not all(key in receipt for key in integrity_keys):
        raise ValueError("The full release receipt is missing plist or launcher integrity records")

    worker = Path(config["EnvironmentVariables"]["PROXY_SDK_WORKER"])
    binary = Path(receipt["binary"])
    candidate = backup / "candidate.plist"
    if (worker != Path(receipt["worker"]) or binary != worker.parents[1] / installed.name
            or Path(config["ProgramArguments"][0]) != installed or not installed.samefile(binary)):
        raise ValueError("The receipt does not match the active full release")
    for path, key, name in ((installed, "binarySha256", "installed binary"),
                            (saved_binary, "previousBinarySha256", "saved binary"),
                            (plist, "plistSha256", "installed plist"),
                            (candidate, "plistSha256", "release plist"),
                            (saved, "previousPlistSha256", "saved plist")):
        if digest(path) != receipt[key]:
            raise ValueError(f"SHA-256 mismatch for the {name}")

    previous = plistlib.loads(saved.read_bytes())
    previous_worker = Path(previous["EnvironmentVariables"]["PROXY_SDK_WORKER"])
    if (previous["Label"] != LABEL or previous["ProgramArguments"] != config["ProgramArguments"]
            or previous_worker.name != "main.mjs" or previous_worker.resolve(strict=True).parents[2] != releases):
        raise ValueError("The saved plist does not describe the previous KnowMe LaunchAgent")
    if (launcher_identity(worker) != receipt["launcherIdentity"]
            or launcher_identity(previous_worker) != receipt["previousLauncherIdentity"]):
        raise ValueError("The Codex launcher no longer matches the verified file and inode")
    subprocess.run(["/usr/bin/plutil", "-lint", str(saved)], check=True, capture_output=True)
    probe_launcher(previous_worker.with_name("codex-exec"))

    install = partial(publish_executable, saved_binary, installed, symlink=saved_binary.is_symlink())
    restore = partial(publish_executable, binary, installed, symlink=installed.is_symlink())
    cutover(plist, saved, candidate, port, install, restore)
    return {"rolledBack": receipt["commit"], "binarySha256": receipt["previousBinarySha256"],
            "worker": str(previous_worker), "launcherInode": receipt["previousLauncherIdentity"]["inode"],
            "health": 200}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--full", action="store_true", help="build and deploy the proxy, console, and locked SDK runtime")
    mode.add_argument("--rollback", type=Path, metavar="RECEIPT", help="restore the previous binary and LaunchAgent")
    parser.add_argument("--launcher", type=Path, help="verified native launcher for an intentional launcher upgrade")
    args = parser.parse_args()
    if args.rollback is not None and args.launcher is not None:
        parser.error("--launcher cannot be used with --rollback")
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
    with (releases / ".worker-deploy.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if config != plistlib.loads(plist.read_bytes()):
            raise RuntimeError("The active LaunchAgent changed during deployment preparation")
        if args.rollback is not None:
            print(json.dumps(rollback(plist, config, releases, args.rollback, port)))
            return
        revision = git("rev-parse", "HEAD").decode().strip()
        kind = "full" if args.full else "workers"
        release = Path(tempfile.mkdtemp(prefix=f"{kind}-{revision[:12]}-", dir=releases))
        print(json.dumps(deploy(plist, config, release, args.launcher, revision, port, full=args.full)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError, KeyError) as error:
        sys.exit(str(error))
