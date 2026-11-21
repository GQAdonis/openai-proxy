"""Exercise launch staging without invoking launchd or provider credentials."""

from pathlib import Path
import os
import plistlib
import runpy
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "deploy-macos-workers.py"
deploy_script = runpy.run_path(str(SCRIPT))
stage_launcher = deploy_script["stage_launcher"]
deploy = deploy_script["deploy"]


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


class LaunchAgentRollback(unittest.TestCase):
    def test_unload_timeout_restarts_the_original_job_if_it_is_gone(self):
        for still_loaded in (False, True):
            with self.subTest(still_loaded=still_loaded), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                original_worker = root / "main.mjs"
                original_worker.touch()
                plist = root / "agent.plist"
                config = {"EnvironmentVariables": {"PROXY_SDK_WORKER": str(original_worker)}}
                original = plistlib.dumps(config)
                plist.write_bytes(original)
                launch = mock.Mock(side_effect=[subprocess.TimeoutExpired([], 20),
                                                mock.Mock(returncode=0 if still_loaded else 113)])
                boot = mock.Mock()
                stage = mock.Mock(return_value=root / "new-main.mjs")
                with (mock.patch.dict(deploy.__globals__, launch=launch, boot=boot, stage_workers=stage),
                      mock.patch.object(Path, "home", return_value=root),
                      mock.patch("subprocess.run"), mock.patch("builtins.print")):
                    with self.assertRaises(subprocess.TimeoutExpired):
                        deploy(plist, config, root / "release", None, "fixture", 8181)
                self.assertEqual(plist.read_bytes(), original)
                if still_loaded:
                    boot.assert_not_called()
                else:
                    boot.assert_called_once_with(plist, f"gui/{os.getuid()}", 8181)


if __name__ == "__main__":
    unittest.main()
