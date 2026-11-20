"""Run in the unprivileged, network-disabled candidate with disposable /tmp."""

import json
import secrets
import sys
import tempfile
import threading
from pathlib import Path

sys.path.insert(0, "/verify/fixture")
from contextlib import contextmanager
from http.server import ThreadingHTTPServer

import auth_native
import recipient
import runner
import runtime

runtime.ps_stamp = (
    lambda pid: Path("/proc", str(pid), "stat")
    .read_text()
    .rsplit(")", 1)[1]
    .split()[19]
)


class H:
    http = runner.Harness.http

    def __init__(self):
        self.root = Path(tempfile.mkdtemp(dir="/tmp", prefix="closed-"))
        self.state = {}
        self.binary = Path("/usr/local/bin/openai-proxy")
        self.keys = {"local": secrets.token_hex(32)}
        self.env = {"PROXY_SDK_BUN": "/usr/local/bin/bun"}
        folder = self.root / "install/lib/openai-proxy"
        folder.mkdir(parents=True)
        (folder / "workers").symlink_to("/opt/proxy/workers")
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), recipient.Handler)
        self.server.root = self.root
        self.upstream = "http://127.0.0.1:" + str(self.server.server_port)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def save(self):
        pass

    def note(self, *args):
        pass


def main():
    h = H()
    results = []
    try:
        if sys.argv[1] == "tool":
            from auth_file_observer import watch_reads

            records = []

            @contextmanager
            def watch(p):
                with watch_reads(p) as observe:
                    assert observe is not None

                    def counted():
                        values = observe()
                        records.append(values)
                        return values

                    yield counted

            auth_native.watch_reads = watch
            for neg in (False, True):
                result = auth_native.tool(h, neg)
                results.append({**result, "inotifyOpenCloseNowrite": records[-1]})
        elif sys.argv[1] == "opencode":
            for neg in (False, True):
                results.append(auth_native.chat(h, "opencode", neg))
        else:
            from real import chat

            results.append(chat(h))
    finally:
        h.server.shutdown()
        h.server.server_close()
    print(json.dumps(results), flush=True)


if __name__ == "__main__":
    main()
