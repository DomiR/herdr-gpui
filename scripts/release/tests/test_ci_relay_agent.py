"""CI builds sheprd-relay-agent as releases do, before merge and only when
what it is built from changes."""

import itertools
import os
import re
import subprocess
import unittest

from test_ci_changes import GitFixture
from test_ci_policy import JOBS


class RelayAgentChanges(GitFixture):
    def test_agent_inputs_run_the_builds(self):
        for path in ("crates/sheprd-relay-agent/src/main.rs", "crates/sheprd-relay-agent/Cargo.toml",
                     "crates/sheprd-relay/src/lib.rs", "crates/sheprd-tunnel/src/lib.rs",
                     "Cargo.lock", "Cargo.toml", "rust-toolchain.toml", ".cargo/config.toml",
                     "scripts/release/build-agent.sh", "scripts/release/common.sh",
                     "scripts/ci-changes.sh", ".github/workflows/ci.yml"):
            self.base = self.git("rev-parse", "HEAD").strip()
            self.commit_path(path)
            self.assertEqual(self.outputs()["relay_agent"], "true", path)

    def test_other_changes_and_unit_tests_skip_them(self):
        for path in ("crates/sheprd-gpui/src/app.rs", "crates/sheprd-remote-control/src/lib.rs",
                     "crates/sheprd-relay-agent/src/tests.rs",
                     "crates/sheprd-relay-agent/src/fleet/tests/paused.rs",
                     "crates/sheprd-relay/README.md", "scripts/release/build-host.sh", "README.md"):
            self.base = self.git("rev-parse", "HEAD").strip()
            self.commit_path(path)
            self.assertEqual(self.outputs()["relay_agent"], "false", path)

    def test_manual_and_unknown_base_run_them(self):
        self.assertEqual(self.outputs("workflow_dispatch")["relay_agent"], "true")
        self.assertEqual(self.outputs("pull_request", "0" * 40)["relay_agent"], "true")


class RelayAgentJob(unittest.TestCase):
    def test_a_fast_pull_request_builds_one_linux_target_and_a_full_one_every_target(self):
        job = JOBS["relay-agent"]
        self.assertIn("        runner: [ubuntu-24.04, ubuntu-24.04-arm, macos-15]\n", job)
        exclude = re.search(r"^        exclude: (.+)$", job, re.M)[1]
        self.assertEqual(exclude, "${{ fromJSON(needs.commits.outputs.full != 'true' && "
                         "'[{\"runner\":\"macos-15\"},{\"runner\":\"ubuntu-24.04-arm\"}]' || '[]') }}")
        # Both Macs' builds share one of the account's few macOS runners.
        self.assertIn('targets="aarch64-apple-darwin x86_64-apple-darwin"', job)
        self.assertIn('targets="$(uname -m)-unknown-linux-musl"', job)
        self.assertIn('build-agent.sh "$target"', job)
        self.assertIn("needs.commits.outputs.relay_agent == 'true'", job)

    def test_the_gate_requires_it_exactly_when_it_runs(self):
        script = JOBS["checks-passed"].split("        run: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines() if line.startswith("          "))

        def gate(event, inputs, result):
            return subprocess.run(
                ["bash", "-e", "-c", script], capture_output=True,
                env={**os.environ, "CODE": "true", "RESULT": "success", "WINDOWS": "success",
                     "NATIVE": "skipped", "NATIVE_INPUTS": "false", "COMMITS": "success", "FULL": "true",
                     "WINDOWS_SCOPE": "true",
                     "PACKAGE_INPUTS": "false", "PACKAGE": "skipped", "EVENT": event,
                     "MACOS_RELEASE": "skipped", "MACOS_INPUTS": "false", "SITE_INPUTS": "false",
                     "SITE": "skipped", "ZIZMOR": "success",
                     "RELAY_AGENT_INPUTS": inputs, "RELAY_AGENT": result},
            ).returncode == 0

        for event, inputs, result in itertools.product(
                ("pull_request", "merge_group", "push"), ("true", "false", ""),
                ("success", "failure", "cancelled", "skipped")):
            runs = inputs == "true" and event in ("pull_request", "merge_group")
            expected = result == ("success" if runs else "skipped")
            self.assertEqual(gate(event, inputs, result), expected, (event, inputs, result))


if __name__ == "__main__":
    unittest.main()
