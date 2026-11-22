#!/bin/sh
# Builds crates/sheprd-fleet-ffi, the fleet log the phone replays its own
# copy with (docs/relay-plan.md, Fleet, phase 5), into
# SheprdRemoteCore/Frameworks/SheprdFleetFFI.xcframework: the iPhone, the
# simulator (Apple silicon and Intel, as a generic simulator build asks for
# both), and the Mac `swift test` runs on. SwiftPM reads
# the framework when it loads the package, so run this before `swift test`
# or an Xcode build; `just ios-test` and `just ios-build` do. The output is
# not committed.
#
# PROFILE=debug builds faster for local work; the default is release.
set -eu
cd "$(dirname "$0")/../.."

profile=${PROFILE:-release}
out=ios/SheprdRemoteCore/Frameworks/SheprdFleetFFI.xcframework
headers=ios/SheprdRemoteCore/FleetFFI/include
targets="aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin"
# The app's deployment target, so the library links without warnings.
export IPHONEOS_DEPLOYMENT_TARGET=26.0
export MACOSX_DEPLOYMENT_TARGET=26.0

if command -v rustup >/dev/null 2>&1; then
    # shellcheck disable=SC2086
    rustup target add $targets >/dev/null
fi

flag=--release
[ "$profile" = release ] || flag=
for target in $targets; do
    cargo build --locked $flag -p sheprd-fleet-ffi --lib --target "$target"
done
lib() { echo "target/$1/$profile/libsheprd_fleet_ffi.a"; }
# One simulator library for both architectures: an XCFramework takes one
# per platform.
simulator=target/ios-simulator/$profile
mkdir -p "$simulator"
lipo -create "$(lib aarch64-apple-ios-sim)" "$(lib x86_64-apple-ios)" -output "$simulator/libsheprd_fleet_ffi.a"
args="-library $(lib aarch64-apple-ios) -headers $headers"
args="$args -library $simulator/libsheprd_fleet_ffi.a -headers $headers"
args="$args -library $(lib aarch64-apple-darwin) -headers $headers"

rm -rf "$out"
mkdir -p "$(dirname "$out")"
# shellcheck disable=SC2086
xcodebuild -create-xcframework $args -output "$out" >/dev/null
echo "Built $out ($profile)"
