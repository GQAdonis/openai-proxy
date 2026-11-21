"""Exercise launch staging without invoking launchd or provider credentials."""

from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "deploy-macos-workers.py"
stage_launcher = runpy.run_path(str(SCRIPT))["stage_launcher"]


class LauncherStaging(unittest.TestCase):
    def test_staged_launcher_keeps_verified_inode_and_runs(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "verified-launcher"
            source.write_text('#!/bin/sh\ntest "$1:$PROXY_CODEX_BINARY" = "exec:/usr/bin/true"\n')
            source.chmod(0o700)
            staged = root / "new-release-launcher"
            stage_launcher(source, staged)
            self.assertTrue(staged.samefile(source))

    def test_launcher_timeout_rejects_staging(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "verified-launcher"
            source.touch(mode=0o700)
            with mock.patch("subprocess.run", side_effect=subprocess.TimeoutExpired([], 8)) as run:
                with self.assertRaisesRegex(RuntimeError, "timed out"):
                    stage_launcher(source, root / "new-release-launcher")
                self.assertEqual(run.call_args.kwargs["timeout"], 8)


if __name__ == "__main__":
    unittest.main()
