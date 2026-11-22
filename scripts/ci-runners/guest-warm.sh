#!/bin/bash
# Runs inside a CI VM, copied there by host.sh: builds main once in the
# runner's own checkout, with the commands and environment ci.yml's jobs on
# this platform use, so the next job finds target/ warm. Keep the commands in
# step with ci.yml: a build with other flags or features warms nothing.
#
# The runner is stopped meanwhile, so no job checks out under the build, and
# restarted however the build ends.
set -uo pipefail

runner="$HOME/actions-runner"
work="$runner/_work/sheprd/sheprd"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_TERM_COLOR=never
if [[ "$(uname -s)" == Darwin ]]; then
  export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/bin:/bin:/usr/sbin:/sbin"
  service() { (cd "$runner" && ./svc.sh "$1" >/dev/null 2>&1); }
  # svc.sh unloads the launchd agent, so a stopped runner is no longer listed.
  running() { launchctl list | grep -q 'actions\.runner\.'; }
else
  export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
  service() { (cd "$runner" && sudo ./svc.sh "$1" >/dev/null 2>&1); }
  # svc.sh records the unit's name; a glob would miss a unit no longer loaded.
  running() { systemctl is-active --quiet "$(cat "$runner/.service")"; }
fi
# Stopping the runner would kill a job it runs; warm it another time.
if pgrep -f '[b]in/Runner\.Worker' >/dev/null; then
  echo "the runner is running a job; not warming"
  exit 0
fi
# A job checking out under the build would break both, so the build waits
# for the service manager to confirm the runner stopped, which a stop issued
# right after a start does not always do.
for _ in 1 2 3 4 5 6; do
  running || break
  service stop
  sleep 5
done
if running; then
  echo "the runner did not stop; not building"
  exit 1
fi
trap 'service start; echo "runner restarted"' EXIT

if [[ ! -d "$work/.git" ]]; then
  mkdir -p "$work" && git -C "$work" init -q &&
    git -C "$work" remote add origin https://github.com/sheprd-labs/sheprd
fi
cd "$work" || exit 1
git fetch -q --no-tags --depth=1 origin main && git checkout -q --force FETCH_HEAD &&
  git clean -q -ffdx -e /target/ || exit 1
echo "main at $(git rev-parse --short HEAD)"
if [[ "$(uname -s)" != Darwin ]]; then
  bash scripts/install-linux-deps.sh >/dev/null 2>&1 || echo "install-linux-deps.sh failed"
fi
rustup show >/dev/null 2>&1

step() {
  local start=$SECONDS code
  "$@" >/dev/null 2>"$HOME/warm-step.err"
  code=$?
  echo "$(((SECONDS - start) / 60))m exit=$code: $*"
  [[ $code == 0 ]] || tail -3 "$HOME/warm-step.err"
}

# The checks job's two legs, whose lint and test passes each platform runs.
step cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings
step cargo test --locked --workspace --no-run
step cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
step cargo test --locked --workspace --all-features --no-run
step cargo build --locked -p sheprd-gpui --all-features
if [[ "$(uname -s)" == Darwin ]]; then
  # build-macos: both release targets and their CLI suites.
  rustup target add x86_64-apple-darwin >/dev/null 2>&1
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    step cargo build --locked --release -p sheprd-gpui --target "$target"
    step cargo test --locked --release -p sheprd-gpui --test cli --target "$target" --no-run
  done
fi
echo "target/ $(du -sh target | cut -f1), $(df -h / | tail -1 | awk '{print $4}') free"
