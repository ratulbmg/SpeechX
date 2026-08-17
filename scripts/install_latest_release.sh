#!/bin/bash
# Downloads the latest published GitHub Release build of SpeechX (the
# .dmg .github/workflows/release.yml attaches on every `vX.Y.Z` tag push)
# and installs it to /Applications — for a fresh Mac that has no dev
# environment set up at all, unlike install_local.sh (which builds from
# a local source checkout instead). Standalone: doesn't need to be run
# from inside a clone of the repo, so it also works piped straight from
# GitHub, e.g.:
#
#   curl -sL https://raw.githubusercontent.com/ratulbmg/SpeechX/main/scripts/install_latest_release.sh | bash
#
# Set GITHUB_TOKEN if the repo is ever made private — unauthenticated
# requests work fine against a public repo but are rate-limited per IP.
set -euo pipefail

REPO="ratulbmg/SpeechX"
API_URL="https://api.github.com/repos/$REPO/releases/latest"

AUTH_HEADER=()
if [ -n "${GITHUB_TOKEN:-}" ]; then
    AUTH_HEADER=(-H "Authorization: Bearer $GITHUB_TOKEN")
fi

echo "Looking up the latest release for $REPO ..."
RELEASE_JSON=$(curl -sL "${AUTH_HEADER[@]}" -H "Accept: application/vnd.github+json" -H "User-Agent: speechx-installer" "$API_URL")

DMG_URL=$(printf '%s' "$RELEASE_JSON" | grep -o '"browser_download_url": *"[^"]*\.dmg"' | sed -E 's/.*"(https[^"]+)"/\1/' | head -n1)
VERSION=$(printf '%s' "$RELEASE_JSON" | grep -o '"tag_name": *"[^"]*"' | sed -E 's/.*"(v[^"]+)"/\1/' | head -n1)

if [ -z "$DMG_URL" ]; then
    echo "Could not find a .dmg asset on the latest release." >&2
    printf '%s\n' "$RELEASE_JSON" | grep -o '"message": *"[^"]*"' >&2 || true
    exit 1
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"; [ -n "${MOUNT_POINT:-}" ] && hdiutil detach "$MOUNT_POINT" -quiet 2>/dev/null || true' EXIT

DMG_PATH="$TMP_DIR/SpeechX.dmg"
echo "Downloading SpeechX ${VERSION:-latest} ..."
curl -sL "$DMG_URL" -o "$DMG_PATH"

echo "Mounting DMG ..."
MOUNT_POINT=$(hdiutil attach "$DMG_PATH" -nobrowse -noautoopen | tail -1 | awk -F'\t' '{print $NF}')

APP_SRC=$(find "$MOUNT_POINT" -maxdepth 1 -iname "*.app" | head -n1)
if [ -z "$APP_SRC" ]; then
    echo "Couldn't find a .app bundle inside the mounted DMG at $MOUNT_POINT." >&2
    exit 1
fi
APP_NAME=$(basename "$APP_SRC")

# Quit a running instance first — installing over its own binary while
# it's open is asking for trouble.
pkill -x speechx 2>/dev/null || true

echo "Installing $APP_NAME to /Applications ..."
rm -rf "/Applications/$APP_NAME"
cp -R "$APP_SRC" "/Applications/$APP_NAME"

hdiutil detach "$MOUNT_POINT" -quiet
MOUNT_POINT=""

echo "Removing quarantine flag ..."
xattr -cr "/Applications/$APP_NAME"

if [ ! -x "/Applications/$APP_NAME/Contents/MacOS/speechx" ]; then
    echo "Install verification failed — /Applications/$APP_NAME is not present after copying." >&2
    exit 1
fi

echo "Installed SpeechX ${VERSION:-} to /Applications/$APP_NAME."
echo "Launching ..."
open "/Applications/$APP_NAME"
