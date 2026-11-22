#!/usr/bin/env bash
# Builds sheprd-relay-agent, which Sheprd installs over SSH on a machine it
# shares as a fleet's plain host (docs/relay-plan.md, Fleet), for one target,
# and writes it to OUTPUT. The sign job lists every target's build in a
# signed manifest (schema 6) and packs them into one archive
# (update-manifest.py pack-host-agents); CI builds the same way when the
# agent's inputs change (scripts/ci-changes.sh, `relay_agent`).
#
# - Linux, x86_64 and aarch64: static musl, so it starts whatever libc the
#   machine has. Build on a machine of that architecture.
# - macOS, arm64 and x86_64: one thin build each, for macOS 11 and later,
#   linking only the system's libraries. The sign job signs it with the
#   release's Developer ID.
#
# Windows and the BSDs get none: the agent's service and installer are POSIX
# shell with systemd or launchd, and its private files rely on Unix modes.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 2 ]] || fail 'Usage: bash build-agent.sh TARGET OUTPUT'
target=$1
new_output "$2"
out=$(cd -- "$(dirname -- "$2")" && pwd)/$(basename -- "$2")
cd "$release_root"
case $target in
x86_64-unknown-linux-musl|aarch64-unknown-linux-musl)
    arch=${target%%-*}
    [[ $(uname -s) == Linux ]] || fail 'Linux required'
    [[ $(uname -m) == "$arch" ]] || fail "Build $target on a $arch machine"
    if ! command -v musl-gcc >/dev/null; then
        sudo apt-get install -y --no-install-recommends musl-tools
    fi
    rustup target add "$target"
    export "CC_${target//-/_}=musl-gcc"
    cargo build --locked --release -p sheprd-relay-agent --target "$target"
    binary=target/$target/release/sheprd-relay-agent
    # No interpreter to look for on the machine it lands on.
    if file -L "$binary" | grep -q 'dynamically linked'; then
        fail 'The agent must be statically linked'
    fi
    "$binary" --version
    cp "$binary" "$out"
    strip "$out"
    ;;
x86_64-apple-darwin|aarch64-apple-darwin)
    [[ $(uname -s) == Darwin ]] || fail 'macOS required'
    case $target in aarch64-*) arch=arm64 ;; *) arch=x86_64 ;; esac
    rustup target add "$target"
    # Older Macs than the app supports may host the agent; its own target
    # directory keeps the app's 14.2 builds from being rebuilt for it.
    MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --locked --release -p sheprd-relay-agent \
        --target "$target" --target-dir target/relay-agent
    binary=target/relay-agent/$target/release/sheprd-relay-agent
    [[ $(lipo -archs "$binary") == "$arch" ]] || fail "The agent must be a thin $arch build"
    # Only the system's own libraries, which every Mac has.
    if otool -L "$binary" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/usr/lib/|/System/Library/)'; then
        fail 'The agent links a library outside the system'
    fi
    # An x86_64 build runs on Apple Silicon only through Rosetta.
    if arch "-$arch" /usr/bin/true 2>/dev/null; then
        "$binary" --version
    fi
    cp "$binary" "$out"
    ;;
*) fail "sheprd-relay-agent is not built for $target" ;;
esac
chmod 755 "$out"
printf '%s\n' "$out"
