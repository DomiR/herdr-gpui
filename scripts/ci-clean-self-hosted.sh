#!/usr/bin/env bash
# A self-hosted runner's checkout, cleaned like a fresh one but for `target/`,
# so the next build reuses what the last one compiled.
#
# Feature sets, profiles, targets, and every dependency version a branch ever
# built pile up in `target/`, so it is pruned before it fills the disk: once
# it passes three quarters of SHEPRD_CI_TARGET_LIMIT_GB (default 80), or the
# disk has less than SHEPRD_CI_MIN_FREE_GB (default 25) free for the build
# about to run, incremental caches go first, then what no build has read in
# SHEPRD_CI_STALE_DAYS (default 3). Cargo rebuilds whatever it finds missing.
# Only when that is not enough is the whole directory dropped.
set -euo pipefail

git clean -ffdx -e /target/
git reset --hard --quiet HEAD

number() {
  [[ "$2" =~ ^[0-9]+$ ]] || { echo "$1 must be a whole number" >&2; exit 1; }
  echo "$2"
}
limit_gb=$(number SHEPRD_CI_TARGET_LIMIT_GB "${SHEPRD_CI_TARGET_LIMIT_GB:-80}")
min_free_gb=$(number SHEPRD_CI_MIN_FREE_GB "${SHEPRD_CI_MIN_FREE_GB:-25}")
stale_days=$(number SHEPRD_CI_STALE_DAYS "${SHEPRD_CI_STALE_DAYS:-3}")
# KB in a "GB" of the settings above; only tests make it smaller.
unit_kb=$(number SHEPRD_CI_UNIT_KB "${SHEPRD_CI_UNIT_KB:-1048576}")

[[ -d target ]] || exit 0
gb() { echo "$(($1 / unit_kb))"; }
used_kb() { du -sk target | cut -f1; }
free_kb() { df -Pk . | awk 'NR == 2 { print $4 }'; }
over() {
  local used free
  used=$(used_kb)
  free=$(free_kb)
  (( used > $1 * unit_kb || free < min_free_gb * unit_kb ))
}
report() { echo "target/ $(gb "$(used_kb)") GB, $(gb "$(free_kb)") GB free: $1"; }

over $((limit_gb * 3 / 4)) || exit 0
report "pruning"
# Incremental caches only speed up rebuilding the workspace's own crates.
find target -type d -name incremental -prune -exec rm -rf {} +
over $((limit_gb * 3 / 4)) || { report "dropped incremental caches"; exit 0; }
# Reading a file updates its access time (at most daily on Linux's relatime),
# so these are artifacts of dependency versions, features, and branches no
# build has used for days. Their fingerprints go with them.
find target -type f -atime "+$stale_days" -delete
find target -mindepth 1 -type d -empty -delete
over "$limit_gb" || { report "dropped what no build read in $stale_days days"; exit 0; }
rm -rf target
echo "target/ removed: still over ${limit_gb} GB, or under ${min_free_gb} GB free"
