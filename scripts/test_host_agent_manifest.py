"""sheprd-relay-agent's builds: their own signed manifest (schema 6),
packed with every build into one archive."""
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("update_manifest", Path(__file__).with_name("update-manifest.py"))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)

RELEASE = "20261120.1"


class HostAgents(unittest.TestCase):
    def write_builds(self, root, targets=updater.HOST_AGENT_TARGETS):
        for target in targets:
            name = updater.host_agent_name(RELEASE, target)
            (root / name).write_bytes(name.encode())

    def test_every_target_is_required_and_listed_in_its_own_schema(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            # A missing Mac build fails the release rather than shipping without it.
            self.write_builds(root, updater.HOST_AGENT_TARGETS[:-1])
            with self.assertRaises(ValueError):
                updater.create(root, RELEASE, host_agents=True)
            self.assertFalse((root / "host-agent-manifest.json").exists())
            self.write_builds(root)
            listed = json.loads(updater.create(root, RELEASE, host_agents=True))
            self.assertEqual(listed["schema"], 6)
            self.assertEqual([asset["target"] for asset in listed["assets"]], list(updater.HOST_AGENT_TARGETS))
            for other in ("host-manifest.json", "host-chat-manifest.json", "update-manifest.json"):
                self.assertFalse((root / other).exists())

    def test_one_family_per_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            for family in (dict(hosts=True), dict(host_chats=True), dict(packages=True), dict(windows=True)):
                with self.assertRaises(ValueError):
                    updater.create(directory, RELEASE, host_agents=True, **family)

    def test_the_schemas_are_distinct(self):
        schemas = [schema for schema, *_ in updater.PACKS.values()]
        self.assertEqual(len(schemas), len(set(schemas)))
        self.assertNotIn(4, schemas)

    def test_the_archive_leads_with_the_signed_manifest_and_holds_each_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            manifest = updater.create(root, RELEASE, host_agents=True)
            (root / "host-agent-manifest.sig").write_bytes(bytes(64))
            output = updater.pack_host_agents(root, RELEASE)
            self.assertEqual(output.name, f"sheprd-relay-agent-{RELEASE}.tar.gz")
            with tarfile.open(output) as archive:
                names = archive.getnames()
                self.assertEqual(names[:2], ["host-agent-manifest.json", "host-agent-manifest.sig"])
                self.assertEqual(sorted(names[2:]),
                                 sorted(f"{target}/sheprd-relay-agent" for target in updater.HOST_AGENT_TARGETS))
                self.assertEqual(archive.extractfile(names[0]).read(), manifest)
                for name in names[2:]:
                    self.assertEqual(archive.getmember(name).mode, 0o755)
            self.assertEqual([path.name for path in root.iterdir()], [output.name])

    def test_a_chat_manifest_is_not_packed_as_the_agents(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            updater.create(root, RELEASE, host_agents=True)
            path = root / "host-agent-manifest.json"
            listed = json.loads(path.read_bytes())
            listed["schema"] = 5
            path.write_text(json.dumps(listed))
            (root / "host-agent-manifest.sig").write_bytes(bytes(64))
            with self.assertRaises(ValueError):
                updater.pack_host_agents(root, RELEASE)
            self.assertFalse((root / f"sheprd-relay-agent-{RELEASE}.tar.gz").exists())

    def test_a_build_changed_after_signing_is_refused_and_kept(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            updater.create(root, RELEASE, host_agents=True)
            (root / "host-agent-manifest.sig").write_bytes(bytes(64))
            build = root / updater.host_agent_name(RELEASE, "aarch64-apple-darwin")
            build.write_bytes(bytes(len(build.read_bytes())))
            with self.assertRaises(ValueError):
                updater.pack_host_agents(root, RELEASE)
            self.assertTrue(build.exists(), "nothing is deleted")

    def test_a_short_signature_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            updater.create(root, RELEASE, host_agents=True)
            (root / "host-agent-manifest.sig").write_bytes(bytes(63))
            with self.assertRaises(ValueError):
                updater.pack_host_agents(root, RELEASE)


if __name__ == "__main__":
    unittest.main()
