#!/bin/bash
# Build, sign, and notarize the universal .dmg for distribution.
# Run only when publishing a release, not during development
# (`npm run tauri dev` / a single-arch `tauri build` is what you want day to day).
#
# Requires a paid Apple Developer account. Set these env vars first:
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: Your Name (TEAMID)"
#   APPLE_ID                 your Apple ID email
#   APPLE_TEAM_ID             your Developer Team ID
#   APPLE_APP_SPECIFIC_PASSWORD   an app-specific password (not your Apple ID password)
set -euo pipefail

: "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY to your Developer ID Application cert}"
: "${APPLE_ID:?Set APPLE_ID to the Apple ID used for notarization}"
: "${APPLE_TEAM_ID:?Set APPLE_TEAM_ID to your Developer Team ID}"
: "${APPLE_APP_SPECIFIC_PASSWORD:?Set APPLE_APP_SPECIFIC_PASSWORD (generate one at appleid.apple.com)}"

cd "$(dirname "$0")/.."

MACOSX_DEPLOYMENT_TARGET=11.0 npm run tauri build -- --target universal-apple-darwin

BUNDLE_DIR="src-tauri/target/universal-apple-darwin/release/bundle"
APP="$BUNDLE_DIR/macos/SpeechX.app"
DMG=$(find "$BUNDLE_DIR/dmg" -name '*.dmg' | head -n1)

echo "Signing $APP ..."
codesign --force --deep --sign "$APPLE_SIGNING_IDENTITY" \
  --options runtime \
  --entitlements src-tauri/Entitlements.plist \
  "$APP"

codesign --verify --deep --strict "$APP"

echo "Notarizing $DMG (this can take a few minutes) ..."
xcrun notarytool submit "$DMG" \
  --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_SPECIFIC_PASSWORD" \
  --wait

echo "Stapling notarization ticket ..."
xcrun stapler staple "$DMG"

echo "Done: $DMG"
spctl --assess --type execute "$APP"
