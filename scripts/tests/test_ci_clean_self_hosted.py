#!/usr/bin/env python3
"""scripts/ci-clean-self-hosted.sh: a self-hosted checkout keeps its target/
until it grows too big or the disk too full, then loses incremental caches,
then what no build read for days, and only then everything."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "ci-clean-self-hosted.sh"
OLD = time.time() - 30 * 86400


class CleanSelfHosted(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        git = ["git", "-C", str(self.root)]
        subprocess.run([*git, "init", "-q"], check=True)
        (self.root / ".gitignore").write_text("target/\n")
        (self.root / "tracked").write_text("kept\n")
        subprocess.run([*git, "add", "."], check=True)
        subprocess.run([*git, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-qm", "i"], check=True)

    def file(self, path, kb, read_long_ago=False):
        path = self.root / "target" / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(os.urandom(kb * 1024))
        if read_long_ago:
            os.utime(path, (OLD, OLD))
        return path

    def clean(self, limit, min_free=0):
        # One "GB" is 100 KB here, so a few small files cross the limits.
        env = {**os.environ, "SHEPRD_CI_UNIT_KB": "100", "SHEPRD_CI_TARGET_LIMIT_GB": str(limit),
               "SHEPRD_CI_MIN_FREE_GB": str(min_free), "SHEPRD_CI_STALE_DAYS": "3"}
        return subprocess.run(["bash", str(SCRIPT)], cwd=self.root, env=env, check=True,
                              capture_output=True, text=True).stdout

    def build(self):
        return {
            "incremental": self.file("debug/incremental/app/cache", 300),
            "stale": self.file("debug/deps/libold.rlib", 300, read_long_ago=True),
            "fresh": self.file("debug/deps/libnew.rlib", 200),
        }

    def test_untracked_files_go_and_a_small_target_stays(self):
        files = self.build()
        (self.root / "stray.log").write_text("x")
        (self.root / "tracked").write_text("edited")
        self.clean(limit=100)
        self.assertFalse((self.root / "stray.log").exists())
        self.assertEqual((self.root / "tracked").read_text(), "kept\n")
        self.assertTrue(all(path.exists() for path in files.values()))

    def test_incremental_caches_go_first(self):
        files = self.build()
        # 800 KB is over three quarters of 10 "GB" (750 KB); 500 KB is not.
        output = self.clean(limit=10)
        self.assertIn("dropped incremental caches", output)
        self.assertFalse(files["incremental"].exists())
        self.assertTrue(files["stale"].exists() and files["fresh"].exists())

    def test_then_what_no_build_read_for_days(self):
        files = self.build()
        output = self.clean(limit=6)
        self.assertIn("dropped what no build read in 3 days", output)
        self.assertFalse(files["incremental"].exists() or files["stale"].exists())
        self.assertTrue(files["fresh"].exists())

    def test_then_everything(self):
        self.build()
        output = self.clean(limit=1)
        self.assertIn("target/ removed", output)
        self.assertFalse((self.root / "target").exists())

    def test_a_full_disk_prunes_a_small_target(self):
        self.build()
        # No disk has a billion "GB" free.
        output = self.clean(limit=100, min_free=10**9)
        self.assertIn("pruning", output)
        self.assertFalse((self.root / "target").exists())

    def test_a_malformed_setting_fails(self):
        env = {**os.environ, "SHEPRD_CI_TARGET_LIMIT_GB": "lots"}
        result = subprocess.run(["bash", str(SCRIPT)], cwd=self.root, env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("SHEPRD_CI_TARGET_LIMIT_GB must be a whole number", result.stderr)


if __name__ == "__main__":
    unittest.main()
