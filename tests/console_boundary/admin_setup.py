"""Packaged OpenCode and client provisioning paths with an independent file oracle."""
import json
import re
from pathlib import Path
from urllib.parse import urlsplit

from admin_common import MissingPeer, case, client, command, http, outputs, private, require, seed, snapshot


OPERATOR = b'{\n  // keep operator comment\n  "model": "openai/gpt-5.5",\n  "plugins": []\n}\n'


def req(h, op, body, statuses=(200,), actor="local"):
    return http(h, "/api/v1/admin/setup/" + op, "POST", body, actor, statuses)


def portable(h, kind, actor="chat"):
    value = http(h, "/api/v1/me/setup/export", "POST",
                 {"integration": kind, "scope": "project", "transport": "stdio"}, actor)
    require(value["status"] == "export" and value["target"] == "visitor" and value["changes"] == [],
            "Visitor export claimed a service-host change")
    require(str(h.root) not in json.dumps(value), "Visitor export disclosed service profile path")
    return value["export"]


def locations(h, scope):
    workspace = private(h, h.env["PROXY_CONSOLE_WORKSPACE"])
    config = workspace if scope == "project" else private(h, h.env["XDG_CONFIG_HOME"]) / "opencode"
    return workspace, config / "opencode.jsonc"


def lifecycle(h, scope, kind, through_cli=False):
    workspace, path = locations(h, scope)
    cwd = workspace if scope == "project" else h.root / "admin-unrelated-cwd"
    cwd.mkdir(mode=0o700, exist_ok=True)
    binary = client(h, "opencode", cwd, "2.3.0")
    port = urlsplit(h.base).port
    request = {"integration": kind, "scope": scope}
    args = [h.binary, "setup", "opencode", "--" + scope, "--port", str(port)]
    if kind == "opencode_plugin":
        args += ["--plugin"]
    with seed(h, path, OPERATOR):
        before = snapshot(h, path, f"{kind} {scope} before")
        if through_cli:
            preview_cli = command(h, [*args, "--dry-run"], workspace)
            require(preview_cli.returncode == 0, "CLI preview failed on the certified host")
            preview = outputs(preview_cli)[0]
        else:
            preview = req(h, "preview", request)
        require(preview["status"] == "ready" and preview["version"] == "2.3.0" and
                Path(preview["path"]).resolve() == path.resolve(), "Preview target/version mismatch")
        require(snapshot(h, path, "after preview") == before, "Preview modified configuration")
        if through_cli:
            applied = command(h, args, workspace)
            require(applied.returncode == 0, "CLI apply failed")
            result = outputs(applied)[-1]
        else:
            result = req(h, "apply", {"id": preview["id"]})
        require(result["status"] == "applied", "Setup did not acknowledge apply")
        try:
            first = snapshot(h, path, "after apply")
            require(first["mode"] == before["mode"], "Config mode changed")
            raw = path.read_text()
            require("// keep operator comment" in raw, "Existing comment disappeared")
            parsed = json.loads(raw.replace("// keep operator comment", ""))
            require(parsed["model"] == "openai/gpt-5.5", "Existing default model changed")
            if kind == "opencode_provider":
                record = parsed["providers"]["openai-proxy"]
                require(record["settings"]["baseURL"] == f"http://127.0.0.1:{port}/api/v1/drivers/codex/v1" and
                        bool(record["models"]), "Static provider endpoint or model inventory mismatch")
                models = command(h, [binary, "models", "openai-proxy"], cwd)
                registered = set(re.findall(rb"openai-proxy/([\w.-]+)", models.stdout))
                require(models.returncode == 0 and
                        registered.intersection(key.encode() for key in record["models"]),
                        "Installed host did not resolve an installed provider model")
            else:
                expected = private(h, parsed["plugins"][0])
                require(expected.is_dir() and (expected / "package.json").is_file(), "Packaged plugin absent")
                models = command(h, [binary, "models", "codex"], cwd)
                require(models.returncode == 0 and re.search(rb"codex/[\w.-]+", models.stdout),
                        "Installed host did not discover a Codex plugin model")
            if through_cli:
                again = command(h, args, workspace)
                require(again.returncode == 0 and not outputs(again)[0]["changes"], "Reapply claimed another edit")
            else:
                require(req(h, "apply", {"id": result["id"]})["status"] == "applied", "Reapply failed")
            require(snapshot(h, path, "after reapply") == first, "Reapplying changed file bytes or mode")
        finally:
            if through_cli:
                undo = command(h, [*args, "--undo", result["id"]], workspace)
                require(undo.returncode == 0 and outputs(undo)[-1]["status"] == "undone", "CLI undo failed")
            else:
                require(req(h, "undo", {"id": result["id"]})["status"] == "undone", "HTTP undo failed")
            require(snapshot(h, path, "after undo") == before, "Undo failed to restore the exact source")
    return {"client": "OpenCode 2.3.0", "scope": scope, "provider": kind, "restored": True}


def invalid(h, scope, content, reason, kind):
    workspace, path = locations(h, scope)
    client(h, "opencode", workspace, "2.3.0")
    with seed(h, path, content):
        before = snapshot(h, path, f"{scope} invalid source")
        refusal = req(h, "preview", {"integration": kind, "scope": scope})
        require(refusal["status"] == "blocked" and reason in refusal["reason"].lower()
                and refusal["export"] and not refusal["changes"], "Refusal did not identify invalid config")
        if kind == "opencode_provider":
            result = command(h, [h.binary, "setup", "opencode", "--" + scope, "--force"], workspace)
            require(result.returncode != 0, "Force accepted invalid config")
        require(before == snapshot(h, path, "after refused setup"), "Invalid config changed")
    return {"writes": 0, "scope": scope}


def conflict(h, undo):
    workspace, path = locations(h, "project")
    client(h, "opencode", workspace, "2.3.0")
    with seed(h, path, OPERATOR):
        preview = req(h, "preview", {"integration": "opencode_provider", "scope": "project"})
        require(preview["status"] == "ready", "Expected ready preview before concurrent edit")
        if undo:
            req(h, "apply", {"id": preview["id"]})
        path.write_bytes(path.read_bytes() + b"\n// independently edited\n")
        before = snapshot(h, path, "independently edited config")
        req(h, "undo" if undo else "apply", {"id": preview["id"]}, statuses=(409,))
        require(before == snapshot(h, path, "after conflict"), "Later user edit was lost")
    return {"writesAfterEdit": 0}


def unsupported_host(h, version):
    # Fileless config/profile outside the test project. A synthetic version
    # respondent exercises only the refusal branch, never a positive host claim.
    directory = h.root / ("admin-host-" + version.replace(".", "-"))
    directory.mkdir(mode=0o700, exist_ok=False)
    fake_bin = directory / "bin"
    fake_bin.mkdir(mode=0o700)
    host = fake_bin / "opencode"
    host.write_text("#!/bin/sh\necho " + version + "\n")
    host.chmod(0o700)
    path = directory / "opencode.jsonc"
    with seed(h, path, OPERATOR):
        before = snapshot(h, path, "unverified host source")
        result = command(h, [h.binary, "setup", "opencode", "--project", "--force"],
                         directory, {**h.env, "PATH": str(fake_bin)})
        require(result.returncode != 0 and outputs(result)[0]["status"] == "blocked", "Unsupported host could write")
        require(snapshot(h, path, "after refused host") == before, "Unsupported host changed config")
    return {"source": "synthetic version respondent", "writes": 0}


def run(h, negative):
    if negative:
        for scope in ("project", "global"):
            for label, content, reason in [
                ("malformed", b'{"preserve-me": ', "json"),
                ("legacy", b'{"provider":{"existing":{}}}', "legacy"),
                ("mixed", b'{"providers":{}, "mcp":{"old":{"type":"local"}}}', "legacy")]:
                case(h, f"{scope} {label} force preserves bytes", lambda: invalid(h, scope, content, reason, "opencode_provider"))
        for undo in (False, True):
            case(h, "conflicting " + ("undo" if undo else "apply"), lambda: conflict(h, undo))
        for version in ("1.9.0", "unknown"):
            case(h, "unsupported version " + version, lambda: unsupported_host(h, version))
        def destination():
            workspace, path = locations(h, "project")
            with seed(h, path, OPERATOR):
                before = snapshot(h, path, "approved destination before")
                outside = h.root / "out-of-project.json"
                require(not outside.exists(), "Unexpected fixture collision")
                req(h, "preview", {"integration": "opencode_provider", "scope": "project",
                                   "path": str(outside)}, statuses=(400, 409))
                require(not outside.exists() and snapshot(h,path,"approved destination after") == before,
                        "An unapproved destination or source changed")
            return {"writes": 0}
        case(h, "unapproved destination", destination)
    else:
        for scope in ("project", "global"):
            for kind in ("opencode_provider", "opencode_plugin"):
                case(h, f"{scope} {kind} discovery and restoration",
                     lambda: lifecycle(h, scope, kind, through_cli=(scope == "global")))
        def export():
            result = portable(h, "opencode_provider")
            require(result["providers"]["openai-proxy"]["models"], "Portable provider lost its model catalogue")
            return {"target": "visitor", "models": len(result["providers"]["openai-proxy"]["models"])}
        case(h, "separate-machine V2 export", export)
