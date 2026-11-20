"""A pinned, external V1 host is a refusal peer only. It never ships with the proxy."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess

from auth_fixture import credential, fixture
from identity import install


VERSION = "1.15.13"
SHA256 = "1cdcb7e426a863528a41ae8f9f6b65470b8395ddd48937229b01299ddc1085f0"
SANDBOX = "/usr/bin/sandbox-exec"


def host_refusal(h):
    selected = os.environ.get("KBD_OPENCODE_V1_BINARY")
    if platform.system() != "Darwin" or platform.machine() != "arm64" or not selected:
        raise FileNotFoundError("pinned external macOS arm64 OpenCode V1 refusal peer unavailable")
    source = Path(selected).resolve(strict=True)
    if not Path(SANDBOX).is_file() or not os.access(source, os.X_OK):
        raise FileNotFoundError("isolated legacy host or macOS sandbox unavailable")
    if any(Path(p).exists() for p in ("/etc/opencode", "/Library/Application Support/opencode",
                                      "/Library/Managed Preferences/ai.opencode.managed.plist")):
        raise FileNotFoundError("system-wide managed OpenCode settings are present")
    with fixture(h, workers=True) as f:
        install(f, credential("owner-a", managed=True))
        peer = f.root / "opencode-v1-probe"
        shutil.copy2(source, peer)
        with peer.open("rb") as data:
            actual = hashlib.file_digest(data, "sha256").hexdigest()
        assert actual == SHA256, "external legacy peer digest differs from the reviewed executable"
        bundle = h.root / "install/lib/openai-proxy/plugin"
        entry = bundle / json.loads((bundle / "package.json").read_text())["exports"]["./server"]
        assert entry.is_file() and entry.resolve().is_relative_to(bundle.resolve()), "installed V2 export missing"
        env = {key: f.env[key] for key in ("HOME", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME",
                                           "PATH", "PROXY_CONSOLE_TOKEN")}
        env.update(PROXY_CONSOLE_URL=f.base, OPENCODE_DISABLE_DEFAULT_PLUGINS="1",
                   OPENCODE_DISABLE_AUTOUPDATE="1", OPENCODE_DISABLE_MODELS_FETCH="1",
                   OPENCODE_DISABLE_CLAUDE_CODE="1", DO_NOT_TRACK="1", CI="1", NO_COLOR="1")
        for key in ("XDG_CACHE_HOME", "XDG_STATE_HOME", "BUN_INSTALL_CACHE_DIR", "TMPDIR"):
            directory = f.root / key.lower()
            directory.mkdir(mode=0o700)
            env[key] = str(directory)
        profile = f.root / "peer.sb"
        profile.write_text(
            '(version 1) (allow default) (deny network*) '
            f'(allow network-outbound (remote tcp "localhost:{f.port}")) '
            '(deny file-write*) (allow file-write* (subpath (param "FIXTURE")) (literal "/dev/null")) '
            '(deny file-read* (subpath (param "OPERATOR_HOME")))'
        )
        config = f.workspace / "opencode.json"
        # A local provider makes a functioning baseline observable without a network catalogue.
        settings = {"provider": {"fixturecontrol": {"npm": "@ai-sdk/openai-compatible",
                    "name": "Fixture", "options": {"baseURL": f.base, "apiKey": "synthetic"},
                    "models": {"canary": {"name": "Canary"}}}}}
        config.write_text(json.dumps(settings))

        def invoke(*args):
            h.count()
            proc = subprocess.run([SANDBOX, "-D", "FIXTURE=" + str(f.root.resolve()),
                                   "-D", "OPERATOR_HOME=" + str(Path.home().resolve()),
                                   "-f", str(profile), str(peer), *args],
                                  cwd=f.workspace, env=env, stdin=subprocess.DEVNULL,
                                  capture_output=True, timeout=35)
            return proc.returncode, proc.stdout, proc.stderr

        code, out, _ = invoke("--version")
        assert code == 0 and out.strip() == VERSION.encode(), "legacy host did not identify the pinned version"
        code, out, _ = invoke("models", "fixturecontrol")
        assert code == 0 and b"fixturecontrol/canary" in out.splitlines(), "isolated legacy host baseline failed"

        settings["plugin"] = [entry.as_uri()]
        config.write_text(json.dumps(settings))
        code, out, err = invoke("--print-logs", "--log-level", "DEBUG", "models", "codex")
        loaded = out + err
        assert os.fsencode(str(entry)) in loaded, "legacy host did not identify the actual V2 export"
        # A syntax, missing dependency, or unrelated startup error cannot prove incompatibility.
        plugin_refusal = rb"plugin export is not a function|requires stable opencode|plugin is not a function"
        def refused(output, marker):
            return re.search(plugin_refusal, output, re.I) or re.search(marker, output, re.I)
        assert not re.search(rb"(?m)^codex/[^\s]+$", out) and refused(loaded, rb"provider not found:\s*codex"), (
            "legacy host did not refuse the provider")
        code, out, err = invoke("--print-logs", "--log-level", "DEBUG", "run", "--format", "json",
                                "--model", "codex/gpt-5.5", "kbd-legacy-refusal")
        assert refused(out + err, rb"model not found:\s*codex/gpt-5\.5|ProviderModelNotFoundError"), (
            "legacy host did not identify an unsupported model or plugin at turn admission")
        assert not f.events("exec") and not f.trace, "legacy turn unexpectedly reached a recipient"
        return {"version": VERSION, "hostSha256": actual, "positiveControl": "fixturecontrol/canary",
                "candidateExportObserved": True, "codexProvider": False, "nativeTurns": 0, "recipientCalls": 0}
