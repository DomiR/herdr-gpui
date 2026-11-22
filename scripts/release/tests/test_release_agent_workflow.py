"""release.yml builds sheprd-relay-agent for every platform it ships, signs
the Mac builds, and publishes them in one archive with a signed manifest."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = (ROOT / ".github/workflows/release.yml").read_text()
SECTIONS = re.split(r"^  ([a-z-]+):\n", WORKFLOW.split("\njobs:\n", 1)[1], flags=re.M)
JOBS = dict(zip(SECTIONS[1::2], SECTIONS[2::2]))
ARCHIVE = "sheprd-relay-agent-${{ needs.validate.outputs.version }}.tar.gz"


class ReleaseAgentWorkflow(unittest.TestCase):
    def test_each_platform_builds_its_agent(self):
        self.assertIn('build-agent.sh "$TARGET" "target/$TARGET/release/sheprd-relay-agent"', JOBS["macos-build"])
        self.assertIn("            target/${{ matrix.target }}/release/sheprd-relay-agent\n", JOBS["macos-build"])
        self.assertIn('build-agent.sh "$HOST_TARGET" "agent/sheprd-relay-agent-$VERSION-$HOST_TARGET"', JOBS["linux"])
        self.assertIn("name: linux-host-agent-${{ matrix.target }}\n", JOBS["linux"])
        # Kept out of the Linux packages the attest job publishes as they are.
        self.assertNotIn("sheprd-relay-agent", JOBS["linux"].split("name: linux-package-")[1])

    def test_the_mac_builds_are_signed_before_their_manifest(self):
        sign = JOBS["sign"]
        for target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
            self.assertIn(f'"host-agents/sheprd-relay-agent-$VERSION-{target}"', sign)
        self.assertLess(sign.index("sign-macos.sh"), sign.index("--host-agents"))
        # Checked again after the Linux builds land in the same folder.
        check = sign.index('grep -Fqx "Authority=$MACOS_SIGNING_IDENTITY"')
        self.assertLess(sign.index("name: linux-host-agent-aarch64-unknown-linux-gnu"), check)
        self.assertLess(check, sign.index('create host-agents "$VERSION" --host-agents'))

    def test_the_sign_job_signs_verifies_and_packs_every_build(self):
        sign = JOBS["sign"]
        for target in ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"):
            self.assertIn(f"name: linux-host-agent-{target}\n          path: host-agents\n", sign)
            self.assertNotIn(f"linux-host-agent-{target}", JOBS["attest"])
        self.assertIn('create host-agents "$VERSION" --host-agents', sign)
        self.assertIn("-in host-agents/host-agent-manifest.json -out host-agents/host-agent-manifest.sig", sign)
        self.assertIn("-in host-agents/host-agent-manifest.json -sigfile host-agents/host-agent-manifest.sig", sign)
        self.assertIn('test "$(wc -c < host-agents/host-agent-manifest.sig | tr -d \' \')" = 64', sign)
        self.assertIn('pack-host-agents host-agents "$VERSION"', sign)
        self.assertIn(f"            dist/{ARCHIVE}\n", sign)
        # The signing key is read once, in the one step that signs.
        self.assertEqual(sign.count("secrets.HERDR_UPDATE_SIGNING_KEY"), 1)

    def test_the_archive_is_attested_and_published(self):
        self.assertIn(f"            dist/{ARCHIVE}\n", JOBS["attest"])
        published = next(line for line in WORKFLOW.splitlines() if line.startswith("          files: Sheprd-"))
        self.assertIn(f" {ARCHIVE}", published)


if __name__ == "__main__":
    unittest.main()
