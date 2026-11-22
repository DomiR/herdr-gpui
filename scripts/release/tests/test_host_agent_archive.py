"""sheprd-relay-agent's archive in the release asset contract."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "artifact-manifest.py"
SPEC = importlib.util.spec_from_file_location("artifact_manifest", SCRIPT)
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)
VERSION = "20261120.1"
ARCHIVE = f"sheprd-relay-agent-{VERSION}.tar.gz"
CHATS = f"sheprd-acp-{VERSION}-linux-musl.tar.gz"


class HostAgentArchive(unittest.TestCase):
    def checksums(self, names):
        directory = Path(self.enterContext(tempfile.TemporaryDirectory()))
        (directory / "SHA256SUMS").write_text("".join(f"{'0' * 64}  {name}\n" for name in names))
        return directory

    def test_a_release_publishes_it_with_its_sidecars_under_the_updater_limit(self):
        names = MANIFEST.asset_names(VERSION)
        self.assertIn(ARCHIVE, MANIFEST.base_names(VERSION))
        for suffix in ("", ".sha256", ".sig", ".crt"):
            self.assertIn(ARCHIVE + suffix, names)
        # SHA256SUMS is the one asset asset_names leaves out.
        self.assertLess(len(names) + 1, 128)
        self.assertNotIn(ARCHIVE, MANIFEST.base_names(VERSION, legacy=True))

    def test_a_release_without_it_still_promotes_but_does_not_verify(self):
        names = [name for name in MANIFEST.asset_names(VERSION) if not name.startswith(ARCHIVE)]
        directory = self.checksums(names)
        with self.assertRaises(ValueError):
            MANIFEST.verify_manifest(directory, VERSION)
        entries = MANIFEST.verify_manifest(directory, VERSION, optional=(ARCHIVE,))
        self.assertEqual(set(entries), set(names))

    def test_an_older_release_is_verified_by_its_own_contract(self):
        self.assertNotIn(ARCHIVE, MANIFEST.base_names(VERSION, host_agents=False))
        names = MANIFEST.asset_names(VERSION, host_agents=False)
        self.assertIn(CHATS, names)
        directory = self.checksums(names)
        entries = MANIFEST.verify_manifest(directory, VERSION, host_agents=False)
        self.assertEqual(set(entries), set(names))
        with self.assertRaises(ValueError):
            MANIFEST.verify_manifest(directory, VERSION)
        # One from before both archives.
        older = MANIFEST.asset_names(VERSION, host_chats=False, host_agents=False)
        self.assertNotIn(CHATS, older)
        entries = MANIFEST.verify_manifest(self.checksums(older), VERSION, host_chats=False, host_agents=False)
        self.assertEqual(set(entries), set(older))

    def test_an_unknown_asset_is_refused_even_when_one_may_be_missing(self):
        names = [*MANIFEST.asset_names(VERSION), "sheprd-relay-agent-x86_64-pc-windows-msvc.exe"]
        with self.assertRaises(ValueError):
            MANIFEST.verify_manifest(self.checksums(names), VERSION, optional=(ARCHIVE,))


if __name__ == "__main__":
    unittest.main()
