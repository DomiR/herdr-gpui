"""Which macOS legs each run gets, and whether our own Macs run them.

Members' code (main, the merge queue, and pull requests from branches of
this repository opened by its owner, members, or collaborators) runs on the
self-hosted Macs CI_MACOS_RUNNER names; a fork's or a bot's pull request
never does. These evaluate the workflow's own expressions with
GitHub's semantics, where `a && b || c` yields a value rather than a boolean.
"""

import ast
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = (ROOT / ".github/workflows/ci.yml").read_text()
SECTIONS = re.split(r"^  ([a-z0-9-]+):\n", WORKFLOW.split("\njobs:\n", 1)[1], flags=re.M)
JOBS = dict(zip(SECTIONS[1::2], SECTIONS[2::2]))
LABELS = '["self-hosted", "macOS", "ARM64", "tart"]'
LINUX_LABELS = '["self-hosted", "Linux", "ARM64", "tart-linux"]'
HOSTED = "macos-15"


def evaluate(text, context):
    """A GitHub expression, `${{ ... }}`, over `context`. Only the subset the
    workflow uses is understood; anything else fails the test."""
    body = re.fullmatch(r"\$\{\{ (.+) \}\}", text)[1]
    body = body.replace("&&", " and ").replace("||", " or ").replace("!=", " != ")

    def value(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return node.value
        if isinstance(node, ast.Attribute):
            return context.get(ast.unparse(node), "")
        if isinstance(node, ast.BoolOp):
            result = None
            for operand in node.values:
                result = value(operand)
                if isinstance(node.op, ast.And) and not result:
                    return result
                if isinstance(node.op, ast.Or) and result:
                    return result
            return result
        if isinstance(node, ast.Compare) and len(node.ops) == 1:
            left, right = value(node.left), value(node.comparators[0])
            if isinstance(node.ops[0], ast.Eq):
                return left == right
            if isinstance(node.ops[0], ast.NotEq):
                return left != right
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == "contains" and len(node.args) == 2):
            return value(node.args[1]) in value(node.args[0])
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id in ("fromJSON", "toJSON") and len(node.args) == 1):
            argument = value(node.args[0])
            return json.loads(argument) if node.func.id == "fromJSON" else json.dumps(argument)
        raise AssertionError(f"Unsupported CI expression: {ast.dump(node)}")

    return value(ast.parse(body, mode="eval").body)


def context(event, fork=False, association="OWNER", full="true", labels=LABELS, linux=LINUX_LABELS):
    return {
        "github.event_name": event,
        "github.repository": "sheprd-labs/sheprd",
        "github.event.pull_request.head.repo.full_name":
            ("contributor/sheprd" if fork else "sheprd-labs/sheprd") if event == "pull_request" else "",
        "github.event.pull_request.author_association": association if event == "pull_request" else "",
        "needs.commits.outputs.full": full,
        "vars.CI_MACOS_RUNNER": labels,
        "vars.CI_LINUX_ARM_RUNNER": linux,
    }


def runs_on(job, values, runner=HOSTED):
    text = re.search(r"^    runs-on: (.+)$", JOBS[job], re.M)[1]
    return evaluate(text, {**values, "matrix.runner": runner})


def legs(values, runner):
    """The feature sets the checks job runs on `runner`."""
    text = re.search(r"^        exclude: (.+)$", JOBS["checks"], re.M)[1]
    excluded = evaluate(text, values)
    return sorted(features for features in ("default", "all")
                  if not any(rule.get("runner") == runner and rule.get("features", features) == features
                             for rule in excluded))


def macos_legs(values):
    return legs(values, HOSTED)


class MacosRunners(unittest.TestCase):
    def test_members_code_runs_on_our_macs(self):
        runs = [context(event) for event in ("push", "merge_group", "workflow_dispatch")]
        runs += [context("pull_request", association=association)
                 for association in ("OWNER", "MEMBER", "COLLABORATOR")]
        for values in runs:
            with self.subTest(values=values):
                self.assertEqual(runs_on("checks", values), json.loads(LABELS))
                self.assertEqual(runs_on("build-macos", values), json.loads(LABELS))

    def test_forks_and_bots_stay_hosted(self):
        # Dependabot's and other apps' branches live here, but their authors
        # are no members: "NONE" or "CONTRIBUTOR".
        for values in (context("pull_request", fork=True),
                       context("pull_request", fork=True, association="MEMBER"),
                       context("pull_request", association="NONE"),
                       context("pull_request", association="CONTRIBUTOR"),
                       context("pull_request", association="FIRST_TIME_CONTRIBUTOR")):
            with self.subTest(values=values):
                self.assertEqual(runs_on("checks", values), HOSTED)
                self.assertEqual(runs_on("build-macos", values), HOSTED)

    def test_the_relay_agent_builds_follow_the_checks(self):
        values = context("pull_request")
        self.assertEqual(runs_on("relay-agent", values), json.loads(LABELS))
        self.assertEqual(runs_on("relay-agent", values, "ubuntu-24.04-arm"), json.loads(LINUX_LABELS))
        self.assertEqual(runs_on("relay-agent", values, "ubuntu-24.04"), "ubuntu-24.04")
        self.assertEqual(runs_on("relay-agent", context("pull_request", fork=True)), HOSTED)
        self.assertIn("run: bash scripts/ci-clean-self-hosted.sh", JOBS["relay-agent"])

    def test_an_unset_variable_keeps_every_run_hosted(self):
        for event in ("pull_request", "push", "merge_group"):
            with self.subTest(event=event):
                values = context(event, labels="")
                self.assertEqual(runs_on("checks", values), HOSTED)
                self.assertEqual(runs_on("build-macos", values), HOSTED)

    def test_arm64_linux_follows_the_same_rule(self):
        self.assertEqual(runs_on("checks", context("pull_request"), "ubuntu-24.04-arm"),
                         json.loads(LINUX_LABELS))
        self.assertEqual(runs_on("checks", context("push"), "ubuntu-24.04-arm"), json.loads(LINUX_LABELS))
        for values in (context("pull_request", fork=True), context("pull_request", association="NONE"),
                       context("push", linux="")):
            with self.subTest(values=values):
                self.assertEqual(runs_on("checks", values, "ubuntu-24.04-arm"), "ubuntu-24.04-arm")
        # The variables are kept apart: one never sends the other's leg away.
        self.assertEqual(runs_on("checks", context("push", labels=""), "ubuntu-24.04-arm"),
                         json.loads(LINUX_LABELS))
        self.assertEqual(runs_on("checks", context("push", linux="")), json.loads(LABELS))

    def test_the_jobs_every_run_waits_for_follow_the_same_rule(self):
        # Hosted runners are shared with the whole account: a members' run
        # must not wait for one before reaching our machines.
        for job in ("zizmor", "commits", "website", "checks-passed"):
            with self.subTest(job=job):
                self.assertEqual(runs_on(job, context("pull_request")), json.loads(LINUX_LABELS))
                self.assertEqual(runs_on(job, context("push")), json.loads(LINUX_LABELS))
                for values in (context("pull_request", fork=True), context("pull_request", association="NONE"),
                               context("push", linux="")):
                    self.assertTrue(runs_on(job, values).startswith("ubuntu-"))
                if job != "checks-passed":
                    self.assertIn("clean: ${{ runner.environment != 'self-hosted' }}", JOBS[job])
                    self.assertIn("run: bash scripts/ci-clean-self-hosted.sh", JOBS[job])

    def test_x86_linux_never_moves(self):
        self.assertEqual(runs_on("checks", context("push"), "ubuntu-24.04"), "ubuntu-24.04")

    def test_every_pull_request_checks_macos(self):
        self.assertEqual(macos_legs(context("pull_request", full="false")), ["default"])
        self.assertEqual(macos_legs(context("pull_request", fork=True, full="false")), ["default"])
        # Every platform: our Macs add the all-features leg; a fork's hosted
        # run, on the account's few hosted macOS runners, does not.
        self.assertEqual(macos_legs(context("pull_request")), ["all", "default"])
        self.assertEqual(macos_legs(context("pull_request", fork=True)), ["default"])
        for event in ("push", "merge_group"):
            self.assertEqual(macos_legs(context(event)), ["all", "default"])

    def test_members_check_linux_on_arm64_and_others_on_x86_64(self):
        # A members' pull request runs Linux on ARM64, our own machines;
        # x86_64 waits for main and the merge queue.
        for full in ("false", "true"):
            values = context("pull_request", full=full)
            self.assertEqual(legs(values, "ubuntu-24.04-arm"), ["all", "default"])
            self.assertEqual(legs(values, "ubuntu-24.04"), [])
        # Anyone else's runs x86_64, adding ARM64 only with every platform.
        for values in (context("pull_request", fork=True, full="false"),
                       context("pull_request", association="NONE", full="false")):
            self.assertEqual(legs(values, "ubuntu-24.04"), ["all", "default"])
            self.assertEqual(legs(values, "ubuntu-24.04-arm"), [])
        self.assertEqual(legs(context("pull_request", fork=True), "ubuntu-24.04-arm"), ["all", "default"])
        for event in ("push", "merge_group"):
            for runner in ("ubuntu-24.04", "ubuntu-24.04-arm"):
                self.assertEqual(legs(context(event), runner), ["all", "default"])


if __name__ == "__main__":
    unittest.main()
