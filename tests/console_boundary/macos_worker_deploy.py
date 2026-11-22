"""Exercise launch staging without invoking launchd or provider credentials."""

from pathlib import Path
import json
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
rollback = deploy_script["rollback"]


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

    def test_failed_full_cutover_restores_the_binary_and_plist(self):
        for symlink in (False, True):
            with self.subTest(symlink=symlink), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                installed = root / ".local/bin/openai-proxy"
                installed.parent.mkdir(parents=True)
                previous = root / "old-binary" if symlink else installed
                previous.write_text("old binary")
                if symlink:
                    installed.symlink_to(previous)
                inode = installed.stat().st_ino
                worker, candidate, codex = (root / name for name in ("main.mjs", "new-binary", "codex"))
                for path in (worker, candidate, codex, worker.with_name("codex-exec")):
                    path.touch()
                plist = root / "agent.plist"
                config = {"ProgramArguments": [str(installed)], "EnvironmentVariables": {
                    "PROXY_SDK_BUN": str(candidate), "PROXY_SDK_WORKER": str(worker),
                }}
                original = plistlib.dumps(config)
                plist.write_bytes(original)

                def launch(*args, **kwargs):
                    receipts = list(root.glob("Library/**/release-receipt.json"))
                    self.assertEqual(len(receipts), 1)
                    receipt = json.loads(receipts[0].read_bytes())
                    self.assertEqual(receipt["binarySha256"], deploy_script["digest"](candidate))
                    self.assertNotIn("health", receipt)

                def boot(*args):
                    if installed.samefile(candidate):
                        self.assertNotEqual(plist.read_bytes(), original)
                        raise RuntimeError("candidate failed health check")
                    self.assertEqual(plist.read_bytes(), original)

                stage = mock.Mock(return_value=(worker, candidate, codex))
                with (mock.patch.dict(deploy.__globals__, launch=launch, boot=mock.Mock(side_effect=boot),
                                      stage_full=stage), mock.patch.object(Path, "home", return_value=root),
                      mock.patch("subprocess.run"), mock.patch("builtins.print")):
                    with self.assertRaisesRegex(RuntimeError, "candidate failed health check"):
                        deploy(plist, config, root / "release", None, "fixture", 8181, full=True)
                self.assertEqual(plist.read_bytes(), original)
                self.assertEqual(installed.stat().st_ino, inode)
                self.assertEqual(installed.is_symlink(), symlink)


class RollbackReceipt(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name).resolve()
        self.releases = self.root / "releases"
        self.installed = self.root / ".local/bin/openai-proxy"
        self.installed.parent.mkdir(parents=True)
        active = self.releases / "active"
        self.worker = active / "workers/main.mjs"
        self.previous = self.releases / "previous/workers/main.mjs"
        for worker in (self.worker, self.previous):
            worker.parent.mkdir(parents=True)
            worker.touch()
        self.launcher = self.worker.with_name("codex-exec")
        self.launcher.write_text("verified launcher")
        os.link(self.launcher, self.previous.with_name("codex-exec"))
        self.binary = active / "openai-proxy"
        self.binary.write_text("active binary")
        self.installed.symlink_to(self.binary)
        backup = self.root / "Library/Application Support/OpenAI Proxy/rollbacks/fixture"
        backup.mkdir(parents=True)
        self.saved_binary = backup / "openai-proxy"
        self.saved_binary.write_text("previous binary")
        label = deploy_script["LABEL"]
        self.plist = self.root / f"{label}.plist"
        self.saved = backup / self.plist.name
        self.candidate = backup / "candidate.plist"
        self.config = {"Label": label, "ProgramArguments": [str(self.installed)],
                       "EnvironmentVariables": {"PROXY_SDK_WORKER": str(self.worker)}}
        previous_config = {**self.config, "EnvironmentVariables": {"PROXY_SDK_WORKER": str(self.previous)}}
        self.plist.write_bytes(plistlib.dumps(self.config))
        self.candidate.write_bytes(self.plist.read_bytes())
        self.saved.write_bytes(plistlib.dumps(previous_config))
        digest = deploy_script["digest"]
        identity = deploy_script["launcher_identity"]
        self.receipt = {"commit": "fixture", "worker": str(self.worker), "binary": str(self.binary),
                        "rollbackBinary": str(self.saved_binary), "rollbackPlist": str(self.saved),
                        "binarySha256": digest(self.binary), "previousBinarySha256": digest(self.saved_binary),
                        "plistSha256": digest(self.candidate), "previousPlistSha256": digest(self.saved),
                        "launcherIdentity": identity(self.worker), "previousLauncherIdentity": identity(self.previous)}
        self.receipt_path = self.root / "receipt.json"
        self.receipt_path.write_text(json.dumps({"deployment": self.receipt}))
        self.launch, self.boot, self.probe = mock.Mock(), mock.Mock(), mock.Mock()

    def restore(self):
        with (mock.patch.dict(rollback.__globals__, launch=self.launch, boot=self.boot, probe_launcher=self.probe),
              mock.patch.object(Path, "home", return_value=self.root), mock.patch("subprocess.run")):
            return rollback(self.plist, self.config, self.releases, self.receipt_path, 8181)

    def test_restores_the_verified_binary_and_plist_without_replacing_the_launcher(self):
        inode = self.launcher.stat().st_ino
        result = self.restore()
        self.assertEqual(result["binarySha256"], self.receipt["previousBinarySha256"])
        self.assertEqual(self.plist.read_bytes(), self.saved.read_bytes())
        self.assertTrue(self.installed.samefile(self.saved_binary))
        self.assertEqual(self.previous.with_name("codex-exec").stat().st_ino, inode)
        self.probe.assert_called_once_with(self.previous.with_name("codex-exec"))

    def test_rejects_a_changed_binary_or_plist_before_stopping_launchd(self):
        for path in (self.installed, self.saved_binary, self.plist, self.candidate, self.saved):
            with self.subTest(path=path.name):
                original = path.read_bytes()
                path.write_bytes(original + b"altered")
                with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                    self.restore()
                path.write_bytes(original)
        self.launch.assert_not_called()

    def test_rejects_a_launcher_copy_with_identical_bytes(self):
        copied = self.previous.with_name("codex-exec")
        copied.unlink()
        copied.write_bytes(self.launcher.read_bytes())
        with self.assertRaisesRegex(ValueError, "launcher no longer matches"):
            self.restore()
        self.launch.assert_not_called()

    def test_legacy_receipt_requires_the_matching_verified_receipt(self):
        keys = ("plistSha256", "previousPlistSha256", "launcherIdentity", "previousLauncherIdentity")
        legacy = {key: value for key, value in self.receipt.items() if key not in keys}
        self.receipt_path.write_text(json.dumps(legacy))
        with self.assertRaisesRegex(ValueError, "predates plist and launcher checksums"):
            self.restore()
        verified = self.saved.with_name("release-receipt.json")
        verified.write_text(json.dumps({**self.receipt, "commit": "other"}))
        with self.assertRaisesRegex(ValueError, "does not match the verified full release"):
            self.restore()
        self.launch.assert_not_called()
        verified.write_text(json.dumps(self.receipt))
        self.assertEqual(self.restore()["rolledBack"], self.receipt["commit"])

    def test_restores_active_release_if_rollback_health_check_fails(self):
        self.boot.side_effect = [RuntimeError("unhealthy"), None]
        with mock.patch("builtins.print"), self.assertRaisesRegex(RuntimeError, "unhealthy"):
            self.restore()
        self.assertTrue(self.installed.samefile(self.binary))
        self.assertEqual(self.plist.read_bytes(), self.candidate.read_bytes())
        self.assertEqual(self.boot.call_count, 2)


if __name__ == "__main__":
    unittest.main()
