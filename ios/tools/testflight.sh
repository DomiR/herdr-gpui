#!/bin/sh
# Archives Herdr Remote and uploads it to TestFlight with an App Store
# Connect API key: Xcode signs automatically with the key (no certificates
# or profiles in the repository) and uploads the export itself.
#
# Needs: APPLE_TEAM_ID, ASC_KEY_ID, ASC_ISSUER_ID, and ASC_KEY_P8 (the .p8
# file's contents). BUILD_NUMBER defaults to the time. Without the key it
# says so and exits 0, so CI passes before the secrets exist.
set -eu
cd "$(dirname "$0")/.."

if [ -z "${ASC_KEY_P8:-}" ] || [ -z "${ASC_KEY_ID:-}" ] || [ -z "${ASC_ISSUER_ID:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
    echo "TestFlight upload skipped: set APPLE_TEAM_ID, ASC_KEY_ID, ASC_ISSUER_ID, and ASC_KEY_P8."
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
key="$work/AuthKey.p8"
umask 077
printf '%s\n' "$ASC_KEY_P8" > "$key"
build="${BUILD_NUMBER:-$(date -u +%Y%m%d%H%M)}"
auth="-allowProvisioningUpdates -authenticationKeyPath $key -authenticationKeyID $ASC_KEY_ID -authenticationKeyIssuerID $ASC_ISSUER_ID"

cat > "$work/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>method</key><string>app-store-connect</string>
<key>destination</key><string>upload</string>
<key>teamID</key><string>$APPLE_TEAM_ID</string>
<key>signingStyle</key><string>automatic</string>
<key>manageAppVersionAndBuildNumber</key><false/>
</dict></plist>
PLIST

xcodegen -q
# shellcheck disable=SC2086
xcodebuild archive -project HerdrRemote.xcodeproj -scheme HerdrRemote \
    -destination 'generic/platform=iOS' -archivePath "$work/HerdrRemote.xcarchive" \
    DEVELOPMENT_TEAM="$APPLE_TEAM_ID" CURRENT_PROJECT_VERSION="$build" \
    $auth -quiet
# The export re-signs with the distribution profile, which sets
# `aps-environment` to production, and uploads to App Store Connect.
# shellcheck disable=SC2086
xcodebuild -exportArchive -archivePath "$work/HerdrRemote.xcarchive" \
    -exportOptionsPlist "$work/ExportOptions.plist" -exportPath "$work/export" \
    $auth -quiet
echo "Uploaded build $build to TestFlight."
