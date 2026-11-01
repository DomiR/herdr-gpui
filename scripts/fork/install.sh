#!/usr/bin/env bash
# Builds this checkout's Herdr UI, signs it with your Apple Development
# certificate, and installs it as the one copy at /Applications/Herdr.app,
# replacing the running app (Herdr and its terminals keep running).
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
#   HERDR_INSTALL_DIR=/Applications             where Herdr.app goes
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
dest="${HERDR_INSTALL_DIR:-/Applications}/Herdr.app"

just bundle release
app="$root/target/release/Herdr.app"

# Every helper beside the executable first, whatever upstream adds, then the
# bundle, whose entitlements allow the microphone a recording needs.
for helper in "$app"/Contents/MacOS/*; do
    [ "$(basename "$helper")" = Herdr ] && continue
    codesign --force --sign "$identity" "$helper"
done
codesign --force --sign "$identity" \
    --entitlements assets/macos/Herdr.entitlements "$app"
codesign --verify --strict "$app"

# Any running Herdr UI quits through its control socket, which the new
# binary speaks as well as the old one. `ps` rather than `pgrep`, which
# does not list the app from every shell (a sandboxed one sees nothing).
running() { ps -axo comm= | grep -qx "$dest/Contents/MacOS/Herdr"; }
if running; then
    "$app/Contents/MacOS/Herdr" app quit || true
    for _ in $(seq 1 40); do
        running || break
        sleep 0.5
    done
    ! running || fail "Herdr UI did not quit; quit it and run this again"
fi

# Copied beside the old one first, so a failed copy leaves it in place.
rm -rf "$dest.new" "$dest.old"
ditto "$app" "$dest.new"
if [ -e "$dest" ]; then
    mv "$dest" "$dest.old"
fi
mv "$dest.new" "$dest"
# The copy it replaced goes to the Trash rather than away for good.
osascript -e "tell application \"Finder\" to delete POSIX file \"$dest.old\"" >/dev/null 2>&1 \
    || rm -rf "$dest.old"

# LaunchServices and the Dock cache icons by bundle id; register this copy
# so they show its own.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$dest"
open "$dest"
printf 'Installed %s, signed by %s\n' "$dest" "$identity"
exit
}
