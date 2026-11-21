"""Verify the native launcher's argument, signal, and timing contracts."""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time


FAKE_CODEX = """\
import pathlib
import signal
import sys
import time

assert sys.argv[1:5] == ["exec", "--ignore-user-config", "--ignore-rules", "--json"]
if sys.argv[5:] == ["-"]:
    assert sys.stdin.read() == "private fixture input"
    print("fixed fixture output")
    sys.exit(23)
assert sys.argv[5:] == ["wait"]
signal.signal(signal.SIGTERM, lambda *args: sys.exit(24))
pathlib.Path("child-ready").touch()
while True:
    time.sleep(0.1)
"""


def verify(executable, root):
    child = root / "fixture-codex"
    child.write_text(f"#!{sys.executable}\n" + FAKE_CODEX)
    child.chmod(0o700)
    timing = root / "timing"
    env = dict(os.environ, PROXY_CODEX_BINARY=str(child), PROXY_CODEX_SPAWN_TIMING=str(timing))
    command = [str(executable), "exec", "--json"]
    result = subprocess.run(command + ["-"], cwd=root, env=env, input="private fixture input",
                            capture_output=True, text=True, timeout=5)
    assert result.returncode == 23 and result.stdout == "fixed fixture output\n" and not result.stderr
    assert re.fullmatch(r"[0-9]+\n", timing.read_text())
    assert timing.stat().st_mode & 0o077 == 0

    with subprocess.Popen(command + ["wait"], cwd=root, env=env, stdin=subprocess.DEVNULL) as process:
        try:
            deadline = time.monotonic() + 5
            while not (root / "child-ready").exists():
                assert time.monotonic() < deadline
                time.sleep(0.05)
            process.terminate()
            assert process.wait(timeout=5) == 24
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as folder:
        verify(Path(sys.argv[1]).resolve(strict=True), Path(folder))
    print("Native Codex launcher preserved isolation flags, exit status, and cancellation")
