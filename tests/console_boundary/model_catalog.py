"""Exercise the installed model list against disposable Codex and HTTP peers.

Run with: python3 -B tests/console_boundary/model_catalog.py /path/to/openai-proxy
"""
import argparse
import json
import secrets
import socket
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from auth_fixture import atomic, credential, digest, fixture, wait_for
from identity import status_json
from runner import Harness


class LocalHarness:
    http = Harness.http

    def __init__(self, root, binary):
        self.root = Path(root)
        self.binary = binary
        self.state = {}
        self.keys = {"local": secrets.token_hex(32)}

    def save(self):
        pass

    def note(self, _name, _value):
        pass


def entry(model, hidden=False):
    return {"model": model, "id": f"picker-{model}", "hidden": hidden}


def models(f, headers=None):
    result = status_json(f.api("/v1/models", headers=headers), 200)
    assert result["object"] == "list"
    for row in result["data"]:
        assert row["object"] == "model" and row["owned_by"] and isinstance(row["created"], int)
    by_id = {row["id"]: row for row in result["data"]}
    assert len(by_id) == len(result["data"]), "model IDs should be unique"
    return by_id


def publish(f, owner):
    atomic(f.store, owner)
    state = status_json(f.api("/api/v1/me/connection/reload", "POST", {}), 200)
    assert state["status"] == "ready"


def subscription(h):
    with fixture(h) as f:
        available = ["gpt-6.2-sol", "gpt-6-astra", "gpt-5.6-sol", "fixture-forward-model"]
        pages = f.home / "catalogue-pages.json"
        atomic(pages, {
            "first": {"data": [entry(available[0]), entry("fixture-hidden", hidden=True)],
                      "nextCursor": "more"},
            "more": {"data": [entry(model) for model in available], "nextCursor": None},
        })
        owner = credential("fixture-owner-a", managed=True)
        publish(f, owner)
        inventory = models(f, headers={"Authorization": ""})
        assert list(inventory) == available, "REST list should match the native catalogue"
        assert inventory["gpt-5.6-sol"]["context_length"] > 0
        assert inventory["gpt-5.6-sol"]["max_output_tokens"] > 0
        assert "context_length" not in inventory["fixture-forward-model"]
        assert "max_output_tokens" not in inventory["fixture-forward-model"]
        assert [event["cursor"] for event in f.events("catalogue")] == [None, "more"]
        assert not f.trace, "model discovery should not send a prompt"

        sent = [*available[:2], "fixture-forward-model"]
        for model in sent:
            stream = model == "fixture-forward-model"
            response = f.api("/v1/chat/completions", "POST", {
                "model": model, "messages": [{"role": "user", "content": "fixture prompt"}],
                "stream": stream,
            })
            if stream:
                code, headers, body = response
                assert code == 200 and "text/event-stream" in headers["content-type"]
                frames = [json.loads(line.removeprefix(b"data: ")) for line in body.splitlines()
                          if line.startswith(b"data: {")]
                assert all(frame["model"] == model for frame in frames)
                assert frames[0]["choices"][0]["delta"]["content"] == "fixture reply"
                assert frames[-1]["choices"][0]["finish_reason"] == "stop"
            else:
                result = status_json(response, 200)
                assert result["choices"][0]["message"]["content"] == "fixture reply"
        assert f.request_models == sent, "REST should preserve each advertised wire ID"
        assert f.trace == [("fixture-owner-a", digest(owner["tokens"]["access_token"]))] * len(sent)

        replacement = "fixture-new-owner-model"
        atomic(pages, {"first": {"data": [entry(replacement)], "nextCursor": None}})
        publish(f, credential("fixture-owner-b", managed=True))
        assert list(models(f)) == [replacement], "a different owner should bypass the previous catalogue cache"
        assert not f.events("exchange"), "discovery should not request a managed refresh"


def api_key(h):
    with fixture(h) as f:
        f.stop_service()
        f.env.update(PROXY_CHAT_AUTH="api_key", PROXY_BACKEND_CREDENTIAL_CLASS="api_key")
        atomic(f.home / "catalogue-pages.json", {
            "first": {"data": [entry("fixture-subscription-only")], "nextCursor": None}
        })
        f.start_service()
        available = models(f)
        assert "gpt-5.5-pro" in available and "fixture-subscription-only" not in available
        assert not f.events("catalogue"), "API key inventory should not spawn native discovery"
        assert not f.trace


def slow_subscription(h):
    with fixture(h) as f:
        f.stop_service()
        available = ["gpt-6.2-sol", "fixture-cold-catalogue-model"]
        atomic(f.store, credential("fixture-slow-owner", managed=True))
        atomic(f.home / "catalogue-pages.json", {
            "first": {"data": [entry(model) for model in available], "nextCursor": None}
        })
        (f.home / "catalogue-delay").write_text("13")
        f.start_service()
        picker = "/api/v1/drivers/codex/v1/models"

        # Let a client leave while Codex is still preparing the catalogue.
        with socket.create_connection(("127.0.0.1", f.port), timeout=5) as client:
            client.sendall((
                f"GET {picker} HTTP/1.1\r\n"
                f"Host: 127.0.0.1:{f.port}\r\n"
                f"Authorization: Bearer {h.keys['local']}\r\n"
                "Connection: close\r\n\r\n"
            ).encode("ascii"))
            wait_for(lambda: f.events("catalogue"))
            client.shutdown(socket.SHUT_RDWR)

        with ThreadPoolExecutor(max_workers=2) as requests:
            protected = requests.submit(f.api, picker)
            public = requests.submit(models, f, {"Authorization": ""})
            inventory = status_json(protected.result(), 200)
            assert [row["id"] for row in inventory["data"]] == available
            assert list(public.result()) == available
        assert len(f.events("catalogue")) == 1, "overlapping callers should reuse the cold native read"
        assert not f.trace, "model discovery should not send a prompt"


def legacy_subscription(h):
    with fixture(h) as f:
        f.stop_service()
        f.env["PROXY_CODEX_BINARY"] = str(f.root / "no-native-cli")
        atomic(f.store, credential("fixture-legacy-owner", managed=True))
        f.start_service()
        assert "gpt-5.6-sol" in models(f), "legacy catalog should still work without a native CLI"
        assert not f.trace


def shared_catalogues(h):
    with fixture(h) as f:
        f.stop_service()
        principals = []
        authorized = {}
        for actor in ["first", "second"]:
            token = secrets.token_hex(32)
            home = f.root / (actor + "-codex")
            workspace = f.root / (actor + "-workspace")
            home.mkdir(mode=0o700)
            workspace.mkdir(mode=0o700)
            (home / ".synthetic-owner").touch()
            atomic(home / "auth.json", credential(actor, managed=True))
            atomic(home / "catalogue-pages.json", {
                "first": {"data": [entry(actor + "-model")], "nextCursor": None}
            })
            principals.append({
                "id": actor, "role": "chat", "tokenSha256": digest(token),
                "codexHome": str(home), "workspace": str(workspace),
            })
            authorized[actor] = {"Authorization": f"Bearer {token}"}

        configuration = f.root / "principals.json"
        atomic(configuration, principals)
        f.env["PROXY_CONSOLE_PRINCIPALS"] = str(configuration)
        f.start_service()
        assert f.api("/v1/models", headers={"Authorization": ""})[0] == 401
        for actor in ["first", "second", "first"]:
            assert list(models(f, headers=authorized[actor])) == [actor + "-model"]
        assert not f.trace


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="oproxy-model-catalog-") as root:
        harness = LocalHarness(root, args.binary.resolve(strict=True))
        subscription(harness)
        slow_subscription(harness)
        api_key(harness)
        legacy_subscription(harness)
        shared_catalogues(harness)
    print("Native catalogue, slow-read coalescing, wire model IDs, owner isolation, and static fallback passed.")
