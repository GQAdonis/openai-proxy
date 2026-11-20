"""Bounded JSON-lines observer for a single disposable child process."""
import contextlib
import hashlib
import json
import os
import select
import signal
import subprocess
import time

from admin_common import note, private, require


class Peer:
    def __init__(self, h, child):
        self.h = h
        self.child = child
        self.buffer = b""
        self.sequence = 0
        self.notifications = []

    def send(self, method, params, ident=None):
        req = {"jsonrpc":"2.0","method":method,"params":params}
        if ident is not None:
            req["id"] = ident
        self.child.stdin.write(json.dumps(req).encode() + b"\n")
        self.child.stdin.flush()

    def call(self, method, params, error=False):
        self.sequence += 1
        self.send(method, params, self.sequence)
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            while b"\n" not in self.buffer:
                readable, _, _ = select.select([self.child.stdout], [], [], max(0,deadline-time.monotonic()))
                require(readable, "Timeout waiting for client reply to " + method)
                data = os.read(self.child.stdout.fileno(),65536)
                require(data, "Client exited before reply to " + method)
                self.buffer += data
                require(len(self.buffer) < 4_000_000, "Oversized protocol response")
            line, self.buffer = self.buffer.split(b"\n",1)
            message = json.loads(line)
            require(all(key.encode() not in line for key in self.h.keys.values()), "Protocol output disclosed a synthetic key")
            if message.get("id") != self.sequence or "method" in message:
                self.notifications.append(message)
                require(len(self.notifications) < 1024, "Too many notifications before reply")
                continue
            note(self.h, "protocol reply", {"method":method,"id":self.sequence,
                        "sha256":hashlib.sha256(line).hexdigest(), "error":"error" in message})
            require(("error" in message) == error, "Unexpected protocol result for " + method)
            return message["error" if error else "result"]
        raise AssertionError("Client timed out waiting for " + method)


@contextlib.contextmanager
def process(h, argv, cwd, env):
    child = subprocess.Popen(list(map(str,argv)),cwd=private(h,cwd),env=env,stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
    try:
        note(h, "isolated protocol child", {"pid":child.pid,"argv":list(map(str,argv))})
        yield Peer(h,child)
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid,signal.SIGTERM)
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid,signal.SIGKILL)
                child.wait(timeout=2)
        child.stdout.close()
        note(h, "protocol child exit", {"pid":child.pid,"code":child.returncode})
