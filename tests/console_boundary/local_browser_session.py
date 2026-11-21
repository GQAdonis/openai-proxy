"""Verify opt-in local console access on a packaged binary and disposable profiles."""
import contextlib
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

from runtime import directory, origin, port, sanitized, wait_http


@contextlib.contextmanager
def service(binary, root, *, automatic=False, shared=False, host="127.0.0.1",
            configure_origin=False):
    env = sanitized(root)
    workspace = directory(root / "workspace")
    state = directory(root / "state")
    number = port()
    base = origin(number)
    env.update(PROXY_CONSOLE_WORKSPACE=str(workspace), PROXY_CONSOLE_DATA=str(state))
    token = "fixture-console-token-only-for-a-disposable-profile"
    if automatic:
        env["PROXY_CONSOLE_LOCAL_AUTO_SESSION"] = "1"
    if configure_origin:
        env["PROXY_CONSOLE_ORIGIN"] = base
    if shared:
        principals = root / "principals.json"
        principals.write_text(json.dumps([{
            "id": "operator", "role": "admin",
            "tokenSha256": hashlib.sha256(token.encode()).hexdigest(),
            "workspace": str(workspace), "codexHome": env["CODEX_HOME"],
        }]))
        env["PROXY_CONSOLE_PRINCIPALS"] = str(principals)
    with subprocess.Popen(
        [str(binary), "serve", "--host", host, "--port", str(number)],
        env=env, cwd=workspace, stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ) as process:
        try:
            wait_http(base, process)
            yield base, state, token
        finally:
            process.terminate()
            process.wait(timeout=10)


def verify(binary, root, pw, chrome):
    request = pw.request.new_context()
    try:
        with service(binary, root / "automatic", automatic=True) as (base, state, _):
            session = base + "/api/v1/session"
            assert not (state / "pairing-token").exists()
            assert request.get(session).status == 401
            assert request.get(base + "/api/v1/admin/settings").status == 401
            assert request.post(session, data={}, headers={"Origin": base}).status == 403
            assert request.post(session, data={}, headers={
                "Origin": origin(port()), "Sec-Fetch-Site": "same-site",
            }).status == 403
            with chrome.new_context(service_workers="block") as context:
                page = context.new_page()
                page.goto(base + "/ui/")
                expect(page.get_by_role("heading", name="Conversation", exact=True)).to_be_visible()
                expect(page.get_by_label("Console access code")).to_have_count(0)
                page.get_by_role("link", name="Settings", exact=True).click()
                expect(page.get_by_role("heading", name="My connection")).to_be_visible()
                assert page.request.get(session).json()["automatic"]
                assert page.request.delete(session, headers={"Origin": base}).status == 403
                context.clear_cookies()
                page.reload()
                expect(page.get_by_role("heading", name="My connection")).to_be_visible()
        for shared in (False, True):
            with service(binary, root / ("shared" if shared else "paired"),
                         automatic=shared, shared=shared) as (base, state, token):
                session = base + "/api/v1/session"
                headers = {"Origin": base, "Sec-Fetch-Site": "same-origin"}
                assert request.post(session, data={}, headers=headers).status == 401
                if not shared:
                    token = (state / "pairing-token").read_text().strip()
                response = request.post(session, data={"token": token}, headers=headers)
                assert response.status == 200 and not response.json()["automatic"]
        with service(binary, root / "wildcard", automatic=True, host="0.0.0.0") as (base, _, _):
            assert request.post(base + "/api/v1/session", data={}, headers={
                "Origin": base, "Sec-Fetch-Site": "same-origin",
            }).status == 503
        with service(binary, root / "configured", automatic=True,
                     configure_origin=True) as (base, _, _):
            session = base + "/api/v1/session"
            headers = {"Origin": base, "Sec-Fetch-Site": "same-origin"}
            assert request.post(session, data={}, headers={
                **headers, "Host": "localhost:" + base.rsplit(":", 1)[-1],
            }).status == 403
            assert request.post(session, data={}, headers=headers).status == 200
    finally:
        request.dispose()


if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as path, sync_playwright() as playwright:
        with playwright.chromium.launch(channel="chrome", headless=True) as browser:
            verify(Path(sys.argv[1]).resolve(strict=True), Path(path), playwright, browser)
    print("Automatic local browser access and authenticated deployment modes passed")
