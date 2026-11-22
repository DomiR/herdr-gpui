#!/bin/bash
# Runs on a Mac mini, copied there by runners.sh: applies each action to every
# CI VM the host keeps running through a LaunchAgent. macOS's bash 3.2.
set -euo pipefail
export PATH="/opt/homebrew/bin:$PATH"

here="$(cd "$(dirname "$0")" && pwd)"
uid="$(id -u)"
# The warm build runs detached inside the VM and logs there: a `tart exec`
# session can drop mid-build, and the guest agent then kills what it ran,
# before the build could restart the runner.
guest_log='$HOME/sheprd-ci-warm.log'

vms() {
  local plist
  for plist in "$HOME"/Library/LaunchAgents/com.sheprd.tartvm.*.plist; do
    [[ -f "$plist" ]] || continue
    plist="${plist##*/com.sheprd.tartvm.}"
    echo "${plist%.plist}"
  done
}

# darwin or linux, from `tart get`'s second line.
os_of() { tart get "$1" | awk 'NR == 2 { print $1 }'; }

# Anchored, so it never matches the pgrep itself.
warming() { tart exec "$1" pgrep -f '^/bin/bash /tmp/guest-warm.sh' >/dev/null 2>&1; }

warm_log() { tart exec "$1" /bin/bash -c "cat $guest_log 2>/dev/null" 2>/dev/null || true; }

# Waits for the VM's guest agent, which `tart exec` needs. An agent that died
# (a guest short of memory loses it) stays dead while the VM runs, so a VM
# that does not answer within a minute is restarted once through its agent.
wait_up() {
  local attempt
  for attempt in $(seq 1 100); do
    tart exec "$1" /usr/bin/true 2>/dev/null && return 0
    if [[ "$attempt" == 20 ]]; then
      echo "$1: guest agent not answering; restarting the VM"
      launchctl kickstart -k "gui/$uid/com.sheprd.tartvm.$1"
    fi
    sleep 3
  done
  echo "$1: not answering" >&2
  return 1
}

copy_in() {
  local file
  for file in guest-setup.sh guest-warm.sh; do
    tart exec -i "$1" /bin/bash -c "cat > /tmp/$file" < "$here/$file"
  done
}

resize() {
  local vm="$1" cpu memory disk label="com.sheprd.tartvm.$1"
  if [[ "$(os_of "$vm")" == darwin ]]; then
    cpu="${MACOS_CPU:-5}" memory="${MACOS_MEMORY_MB:-14336}" disk="${MACOS_DISK_GB:-140}"
  else
    cpu="${LINUX_CPU:-4}" memory="${LINUX_MEMORY_MB:-8192}" disk="${LINUX_DISK_GB:-100}"
  fi
  if tart get "$vm" | awk -v c="$cpu" -v m="$memory" -v d="$disk" 'NR == 2 && $2 == c && $3 == m && $4 == d { found = 1 } END { exit !found }'; then
    echo "$vm: already $cpu cpu, $memory MB, $disk GB"
    return 0
  fi
  launchctl bootout "gui/$uid/$label" 2>/dev/null || true
  local _
  for _ in $(seq 1 60); do
    tart list | grep -q " $vm .*stopped" && break
    sleep 2
  done
  # A disk only grows; the guest grows its file system on the next boot.
  tart set "$vm" --cpu "$cpu" --memory "$memory" --disk-size "$disk"
  launchctl bootstrap "gui/$uid" "$HOME/Library/LaunchAgents/$label.plist"
  wait_up "$vm"
  echo "$vm: $cpu cpu, $memory MB, $disk GB"
}

setup() {
  wait_up "$1"
  copy_in "$1"
  echo "$1: $(tart exec "$1" /bin/bash /tmp/guest-setup.sh </dev/null 2>&1 | tail -1)"
}

warm() {
  local vm="$1"
  wait_up "$vm"
  copy_in "$vm"
  # Started detached inside the VM, so neither this session nor the guest
  # agent's ends it; the guest script restarts the runner when it ends.
  # setsid on Linux leaves the agent's session; macOS's agent lets nohup be.
  tart exec "$vm" /bin/bash -c "
    if command -v setsid >/dev/null; then detach='setsid -f'; else detach=nohup; fi
    \$detach /bin/bash /tmp/guest-warm.sh </dev/null >$guest_log 2>&1 &"
  echo "$vm: warming, log ~/sheprd-ci-warm.log in the VM"
}

status() {
  local vm="$1" size runner last log
  size="$(tart get "$vm" | awk 'NR == 2 { print $2 " cpu, " $3 " MB, " $4 " GB, " $NF }')"
  # Reading a log must not end the script under `set -e`: none may exist yet.
  log="$(warm_log "$vm")"
  if warming "$vm"; then
    last="warming: $(grep -E '^[0-9]+m exit=' <<<"$log" | tail -1 || true)"
  elif [[ -n "$log" ]]; then
    last="$(grep -E '^(main at|[0-9]+m exit=[1-9]|target/|the runner)' <<<"$log" | tr '\n' ' ' || true)"
  else
    last="never warmed"
  fi
  # Asks the service manager, not svc.sh, whose status quotes old log lines.
  # svc.sh stop unloads the macOS agent, so an unlisted runner is stopped.
  runner="$(tart exec "$vm" /bin/bash -c '
    if [[ "$(uname -s)" == Darwin ]]; then
      launchctl list | awk "/actions\.runner\./ { print (\$1 == \"-\") ? \"stopped\" : \"running\"; found = 1 } END { if (!found) print \"stopped\" }"
    else
      systemctl is-active "$(cat ~/actions-runner/.service)" 2>/dev/null | sed "s/^inactive$/stopped/; s/^active$/running/"
    fi' 2>/dev/null || echo unreachable)"
  echo "$vm: $size; runner $runner; $last"
}

# A VM skipped because it was warming stays skipped for the later actions
# too: its build may end meanwhile, and `setup warm` would then warm it again.
skipped=" "
for action in "$@"; do
  for vm in $(vms); do
    if [[ "$action" != status && "$action" != wait ]]; then
      [[ "$skipped" == *" $vm "* ]] && continue
      if warming "$vm"; then
        echo "$vm: a warm build is running; skipped"
        skipped+="$vm "
        continue
      fi
    fi
    case "$action" in
      resize | setup) "$action" "$vm" & ;;
      warm) warm "$vm" ;;
      status) status "$vm" ;;
      wait) while warming "$vm"; do sleep 30; done ;;
      *) echo "unknown action: $action" >&2; exit 64 ;;
    esac
  done
  wait
  [[ "$action" == wait ]] && for vm in $(vms); do status "$vm"; done
done
exit 0
