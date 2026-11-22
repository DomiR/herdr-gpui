#!/usr/bin/env bash
# Manage the self-hosted CI runners: Tart VMs on Mac minis reached over SSH
# (docs/self-hosted-runners.md). Run it from any machine with SSH access to
# the hosts; each host does its own VMs in parallel.
#
#   scripts/ci-runners/runners.sh [ACTION...] HOST...
#
# Actions, default `setup warm`:
#   resize  grow each VM (stops and restarts it through its LaunchAgent)
#   setup   install Rust and the packages CI needs, and the runner's PATH
#   warm    build main once in the runner's checkout, runner stopped meanwhile
#   wait    wait for every warm build to end, then print `status`
#   status  each VM's size, runner, and last warm build
#
# A host's VMs are those with a LaunchAgent com.sheprd.tartvm.<vm>.plist.
# Sizes come from MACOS_CPU, MACOS_MEMORY_MB, MACOS_DISK_GB, LINUX_CPU,
# LINUX_MEMORY_MB, and LINUX_DISK_GB, passed on to each host.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
actions=()
hosts=()
for argument in "$@"; do
  case "$argument" in
    resize | setup | warm | wait | status) actions+=("$argument") ;;
    -h | --help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $argument" >&2; exit 64 ;;
    *) hosts+=("$argument") ;;
  esac
done
[[ ${#hosts[@]} -gt 0 ]] || { echo "usage: $0 [resize|setup|warm|wait|status]... HOST..." >&2; exit 64; }
[[ ${#actions[@]} -gt 0 ]] || actions=(setup warm)

sizes=()
for name in MACOS_CPU MACOS_MEMORY_MB MACOS_DISK_GB LINUX_CPU LINUX_MEMORY_MB LINUX_DISK_GB; do
  if [[ -n "${!name:-}" ]]; then
    [[ "${!name}" =~ ^[0-9]+$ ]] || { echo "$name must be a whole number" >&2; exit 64; }
    sizes+=("$name=${!name}")
  fi
done

remote=/tmp/sheprd-ci-runners
run_host() {
  local host="$1"
  ssh -o BatchMode=yes "$host" "mkdir -p $remote" &&
    scp -q "$here/host.sh" "$here/guest-setup.sh" "$here/guest-warm.sh" "$host:$remote/" &&
    ssh -o BatchMode=yes "$host" "env ${sizes[*]:-} /bin/bash $remote/host.sh ${actions[*]}" 2>&1 |
    sed "s/^/[$host] /"
}

pids=()
for host in "${hosts[@]}"; do
  run_host "$host" &
  pids+=("$!")
done
status=0
for pid in "${pids[@]}"; do
  wait "$pid" || status=1
done
exit "$status"
