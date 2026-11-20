"""Real embedded host package discovery. Subscriber usage belongs to a separate observation."""
import json
import tarfile
import shutil
import subprocess
from pathlib import Path

from auth_fixture import atomic, credential, fixture
from auth_native import installed
from identity import install


def call_host(h, f, mode):
    workers = h.root / "install/lib/openai-proxy/workers"
    bundle = workers.parent / "plugin"
    project = f.workspace
    (project / "host-config").mkdir()
    project_package = project / "plugin"
    # Host receives the project-relative directory in one case and the installed package
    # directory in the other. Each resolves the actual build's server export.
    spec = str(bundle)
    if mode == "project":
        shutil.copytree(bundle, project_package, ignore=shutil.ignore_patterns("node_modules"))
        (project_package / "node_modules").symlink_to(bundle / "node_modules", target_is_directory=True)
        spec = "./plugin"
    if mode == "installed":
        # Install the artifact created by npm pack, including only that archive's
        # regular package files. The build staged its locked runtime dependencies.
        tarballs = list(h.root.glob("prometheus-ags-opencode-codex-proxy-*.tgz"))
        if len(tarballs) != 1:
            raise FileNotFoundError("expected a single frozen V2 npm package archive")
        installed = project / "node_modules/@prometheus-ags/opencode-codex-proxy"
        installed.mkdir(parents=True)
        with tarfile.open(tarballs[0], "r:gz") as archive:
            for member in archive:
                path = Path(member.name)
                assert not path.is_absolute() and ".." not in path.parts and path.parts[:1] == ("package",)
                if member.isdir():
                    continue
                assert member.isfile() and member.size <= 8_000_000, "unexpected packed package member"
                dest = installed.joinpath(*path.parts[1:])
                dest.parent.mkdir(parents=True, exist_ok=True)
                with archive.extractfile(member) as packed:
                    dest.write_bytes(packed.read())
        package = json.loads((installed / "package.json").read_text())
        assert package["name"] == "@prometheus-ags/opencode-codex-proxy" and package["dependencies"]["@opencode/plugin"] == "2.3.0"
        assert package["exports"]["./server"] == "./dist/index.js" and (installed / "dist/index.js").is_file()
        (installed / "node_modules").symlink_to(bundle / "node_modules", target_is_directory=True)
        spec = str(installed)
    config = {"plugins": [spec]}
    if mode == "override":
        config["providers"] = {"codex": {"name": "Operator fixture", "package": "@opencode/ai/providers/openai-compatible",
                                          "settings": {"baseURL": f.base + "/api/v1/drivers/codex/v1"}}}
    (project / "opencode.json").write_text(json.dumps(config))
    install(f, credential("owner-a", managed=True))
    atomic(f.home / "next-generation", credential("owner-a", managed=True))
    (f.home / "next-model").write_text("gpt-5.4")
    env = dict(f.env, PROXY_CONSOLE_URL=f.base, HTTP_PROXY="http://127.0.0.1:1",
               HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1",
               XDG_CACHE_HOME=str(f.root / "cache"), XDG_STATE_HOME=str(f.root / "state"),
               BUN_INSTALL_CACHE_DIR=str(f.root / "bun-cache"))
    script = Path(__file__).with_name("auth_plugin_host.mjs")
    exe = h.env["PROXY_SDK_BUN"]
    args = [exe, str(script), str(workers), str(project), mode, str(bundle)]
    result = subprocess.run(args, env=env, cwd=project, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=40, check=False)
    assert result.returncode == 0, f"installed plugin/host oracle failed in {mode} (exit {result.returncode})"
    assert result.stdout.strip(), "no host observation"
    report = json.loads(result.stdout.strip().splitlines()[-1])
    if mode == "version-guard":
        assert report == {"controlledVersionsRejected": 4}
    elif mode == "override":
        assert report == {"preservedOverride": True}
    else:
        assert report == {"hostVersion": "2.3.0", "provider": "codex", "changedModel": "gpt-5.4"}
    return report


def plugin(h, negative):
    workers = h.root / "install/lib/openai-proxy/workers"
    required = (workers / "node_modules/@opencode/sdk/package.json", workers.parent / "plugin/dist/index.js")
    if any(not p.is_file() for p in required) or not h.env.get("PROXY_SDK_BUN"):
        h.block("packaged V2 plugin and pinned native host", "installed host, built plugin export or Bun unavailable")
    elif any(Path(p).exists() for p in ("/etc/opencode", "/Library/Application Support/opencode",
                                        "/Library/Managed Preferences/ai.opencode.managed.plist")):
        h.block("native host profile", "system OpenCode profile would enter the disposable host")
    else:
        for mode in (("version-guard",) if negative else ("project", "installed", "override")):
            def test(mode=mode):
                with fixture(h, workers=True) as f:
                    return call_host(h, f, mode)
            h.check("installed plugin " + mode, test)
    if negative:
        h.block("actual V1-host registration denial", "version-guard fixture provides no independent V1 host")
        if installed(h, "codex"):
            for word, expected in (("KBD_DENIED", 403), ("KBD_QUOTA", 429)):
                def deny(word=word, expected=expected):
                    with fixture(h, workers=True) as f:
                        install(f, credential("owner-a", managed=True))
                        req = {"model": "gpt-5.5", "messages": [{"role": "user", "content": word}]}
                        code, _, _ = f.api("/api/v1/drivers/codex/v1/chat/completions", "POST", req)
                        assert len(f.events("exec")) == 1 and not f.trace, "denied subscription turn caused a second effect"
                        assert code == expected, f"expected attributed denial HTTP {expected}, received {code}"
                        return {"synthetic_code": word, "service_http": code, "native_execs": 1}
                h.check("synthetic subscription projection " + word, deny)
        else:
            h.block("synthetic model denial", "Codex SDK worker unavailable")
        h.block("actual subscriber quota or denied entitlement", "requires separately consented controlled subscriber")
    else:
        h.block("real highest-tier subscription turn and independent allowance attribution",
                "requires current consent and correlated provider usage evidence")
