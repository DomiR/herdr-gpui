"""sign-macos.sh signs executables shipped outside the app in place:
sheprd-relay-agent's Mac builds."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]


class SignStandalone(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="herdr-sign-test-")
        self.addCleanup(temp.cleanup)
        self.work = Path(temp.name)
        tools = self.work / "tools"
        tools.mkdir()
        mock = tools / "mock.py"
        mock.write_bytes((SCRIPTS / "tests/mock-tool.py").read_bytes())
        mock.chmod(0o755)
        for tool in ["uname", "security", "openssl", "base64", "plutil", "lipo", "ditto", "xcrun", "hdiutil",
                     "codesign", "spctl"]:
            (tools / tool).symlink_to(mock)
        self.log = self.work / "log"
        self.env = {
            "PATH": str(tools) + os.pathsep + os.environ["PATH"], "HOME": str(self.work), "MOCK_LOG": str(self.log),
            "MACOS_CERTIFICATE_P12_BASE64": "ZHVtbXk=", "MACOS_CERTIFICATE_PASSWORD": "dummy",
            "APPLE_API_PRIVATE_KEY": "dummy p8", "APPLE_API_KEY_ID": "dummy", "APPLE_API_ISSUER_ID": "dummy",
            "MACOS_SIGNING_IDENTITY": "Developer ID Application: Dummy", "APPLE_RETRY_DELAY": "0",
        }
        self.app = self.work / "unsigned.app"
        (self.app / "Contents/MacOS").mkdir(parents=True)
        for name in ("sheprd", "sheprd-acp", "sheprd-remote-control", "sheprd-automate", "sheprd-nearby",
                     "herdr-core", "herdr-acp", "herdr-remote-control", "herdr-automate", "herdr-nearby"):
            (self.app / "Contents/MacOS" / name).write_text("never run")
        (self.app / "Contents/MacOS/sheprd").chmod(0o755)
        (self.app / "Contents/Resources").mkdir()
        (self.app / "Contents/Resources/THIRD-PARTY-NOTICES.txt").write_text("Third-party license text\n")
        self.out = self.work / "out"
        self.out.mkdir()

    def sign(self, *executables):
        return subprocess.run(
            ["bash", str(SCRIPTS / "sign-macos.sh"), "20260920.3", str(self.app), str(self.out),
             *map(str, executables)],
            env=self.env, capture_output=True, text=True, timeout=30,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_each_executable_is_signed_with_the_runtime_and_verified(self):
        agents = [self.work / f"sheprd-relay-agent-20261120.1-{target}"
                  for target in ("x86_64-apple-darwin", "aarch64-apple-darwin")]
        for agent in agents:
            agent.write_text("never run")
        result = self.sign(*agents)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        for agent in agents:
            signing = [call for call in calls if call[0] == "codesign" and call[-1] == str(agent)]
            self.assertEqual(len(signing), 2, signing)
            self.assertIn("--options", signing[0])
            self.assertEqual(signing[0][signing[0].index("--options") + 1], "runtime")
            self.assertIn("--timestamp", signing[0])
            self.assertNotIn("--entitlements", signing[0])
            # One identity for every release's build, whatever its file name.
            self.assertEqual(signing[0][signing[0].index("--identifier") + 1], "org.sheprd.relay-agent")
            self.assertEqual(signing[0][signing[0].index("--sign") + 1], "Developer ID Application: Dummy")
            self.assertEqual(signing[1][:2], ["codesign", "--verify"])
        # Signed while the keychain still exists, before it is deleted.
        deleted = next(index for index, call in enumerate(calls) if call[:2] == ["security", "delete-keychain"])
        last = max(index for index, call in enumerate(calls) if call[0] == "codesign" and call[-1] in map(str, agents))
        self.assertLess(last, deleted)

    def test_a_missing_or_linked_executable_fails_before_any_keychain(self):
        agent = self.work / "agent"
        result = self.sign(agent)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Regular executable required", result.stderr)
        agent.write_text("never run")
        link = self.work / "link"
        link.symlink_to(agent)
        result = self.sign(link)
        self.assertNotEqual(result.returncode, 0)
        calls = self.calls() if self.log.exists() else []
        self.assertFalse([call for call in calls if call[0] == "security"])

    def test_no_executables_signs_only_the_app(self):
        result = self.sign()
        self.assertEqual(result.returncode, 0, result.stderr)
        standalone = [call for call in self.calls() if call[0] == "codesign"
                      and "Sheprd.app" not in call[-1] and not call[-1].endswith(".dmg")]
        self.assertEqual(standalone, [])


if __name__ == "__main__":
    unittest.main()
