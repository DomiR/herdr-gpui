#!/usr/bin/env bash
# Builds this checkout's Sheprd, signs it with your Apple Development
# certificate, and installs it as the one copy at /Applications/Sheprd.app,
# replacing the running app (Herdr and its terminals keep running). A
# Herdr.app from before the rename is quit and moved to the Trash.
#
# Run it in the plain clone that holds `domir`, after rebuild-domir.sh:
#
#   scripts/fork/rebuild-domir.sh && scripts/fork/install.sh
#
# Why the certificate: a local build is otherwise signed ad hoc, and macOS
# ties Screen Recording and the microphone to that exact binary, so every
# rebuild loses them. Signed with a certificate, the grants follow the
# certificate and survive rebuilds. Local builds carry the version `dev`,
# which the in-app updater never replaces.
#
#   HERDR_SIGN_IDENTITY="Apple Development: …"  the certificate to sign with
#   HERDR_INSTALL_DIR=/Applications             where Sheprd.app goes
#
# The whole script is read before it runs, as rebuild-domir.sh is.
{
set -euo pipefail

fail() { printf 'install: %s\n' "$*" >&2; exit 1; }

root=$(git rev-parse --show-toplevel)
cd "$root"
[ "$(uname -s)" = Darwin ] || fail "installs on macOS only"

identity=${HERDR_SIGN_IDENTITY:-$(security find-identity -v -p codesigning \
    | sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -n 1)}
[ -n "$identity" ] || fail "no Apple Development certificate; set HERDR_SIGN_IDENTITY"
dest="${HERDR_INSTALL_DIR:-/Applications}/Sheprd.app"
# The copy installed before the rename, with the same bundle id.
legacy="${HERDR_INSTALL_DIR:-/Applications}/Herdr.app"

just bundle release
app="$root/target/release/Sheprd.app"

# Every helper beside the executable first, whatever upstream adds, then the
# bundle, whose entitlements allow the microphone a recording needs.
for helper in "$app"/Contents/MacOS/*; do
    [ "$(basename "$helper")" = sheprd ] && continue
    codesign --force --sign "$identity" "$helper"
done
codesign --force --sign "$identity" \
    --entitlements assets/macos/Sheprd.entitlements "$app"
codesign --verify --strict "$app"

# Any running Sheprd quits through its control socket, which the new
# binary speaks as well as the old one. `ps` rather than `pgrep`, which
# does not list the app from every shell (a sandboxed one sees nothing).
# Only the window counts: the same binary also runs agents' commands, such
# as a long `doc watch`, which take arguments and never quit.
running() {
    ps -axo args= | grep -qxE "$dest/Contents/MacOS/sheprd|$legacy/Contents/MacOS/(Herdr|sheprd)"
}
if running; then
    "$app/Contents/MacOS/sheprd" app quit || true
    for _ in $(seq 1 40); do
        running || break
        sleep 0.5
    done
    ! running || fail "Sheprd did not quit; quit it and run this again"
fi

# Copied beside the old one first, so a failed copy leaves it in place.
rm -rf "$dest.new" "$dest.old"
ditto "$app" "$dest.new"
if [ -e "$dest" ]; then
    mv "$dest" "$dest.old"
fi
mv "$dest.new" "$dest"
# The copy it replaced goes to the Trash rather than away for good.
trash() {
    osascript -e "tell application \"Finder\" to delete POSIX file \"$1\"" >/dev/null 2>&1 \
        || rm -rf "$1"
}
trash "$dest.old"
# Two bundles with one id would leave LaunchServices free to open either.
if [ -e "$legacy" ]; then
    trash "$legacy"
fi

# LaunchServices and the Dock cache icons by bundle id; register this copy
# so they show its own.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$dest"
open "$dest"
printf 'Installed %s, signed by %s\n' "$dest" "$identity"
exit
}
