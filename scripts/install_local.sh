#!/bin/bash
# Build SpeechX and install it to /Applications for local testing on this
# Mac — day-to-day dev use, not the notarized release build (see
# release.sh for that, which needs a paid Apple Developer account).
#
# Two things this does that a plain `npm run tauri build` + drag-to-
# Applications does not:
#
# 1. Re-signs the bundle with `codesign --force --deep --sign -` after
#    building. Tauri's own ad-hoc signing step sometimes produces a
#    bundle where the signature claims sealed resources exist but none
#    are recorded (`code has no resources but signature indicates they
#    must be present` — a known, generic macOS bundler quirk, not
#    specific to anything in this project). A plain double-click launch
#    fails against a signature like that. Re-signing the fully-assembled
#    bundle in one pass fixes it.
#
# 2. Removes any quarantine flag so Finder doesn't re-flag it, and prints
#    the one manual step Gatekeeper still requires: even a *correctly*
#    ad-hoc-signed app (no paid Developer ID, no notarization — that's
#    M9, not built yet) is not from an identified developer, so macOS
#    blocks a plain double-click every time regardless. The standard,
#    expected bypass is a right-click → Open the first time — that's not
#    a setup step you're missing, it's how Gatekeeper works for any
#    unsigned app on any Mac.
set -euo pipefail

cd "$(dirname "$0")/.."

npm run tauri build

APP="src-tauri/target/release/bundle/macos/SpeechX.app"

# Right after `tauri build` finishes, the bundle can still be briefly
# locked (DMG creation's `hdiutil` finalizing, Spotlight indexing the
# freshly-created bundle, etc.) — codesign fails with "Operation not
# permitted" against a lock like that. It clears within a second or two,
# so retry a few times instead of aborting on what's usually transient.
echo "Re-signing $APP ..."
signed=false
for attempt in 1 2 3 4 5; do
    if codesign --force --deep --sign - "$APP"; then
        signed=true
        break
    fi
    echo "codesign attempt $attempt failed (likely a transient lock) — retrying in 2s..."
    sleep 2
done
if [ "$signed" != true ]; then
    echo "codesign failed after 5 attempts — not a transient lock, something else is wrong." >&2
    exit 1
fi

echo "Installing to /Applications ..."
rm -rf /Applications/SpeechX.app
cp -R "$APP" /Applications/SpeechX.app
xattr -d com.apple.quarantine /Applications/SpeechX.app 2>/dev/null || true

# Verify the install actually landed instead of trusting the steps above
# ran silently correctly — this is exactly the kind of failure that was
# hard to notice before (a mid-script abort with `/Applications/SpeechX.app`
# just never updated, easy to mistake for the app "disappearing").
if [ ! -x "/Applications/SpeechX.app/Contents/MacOS/speechx" ]; then
    echo "Install verification failed — /Applications/SpeechX.app is not present after copying." >&2
    exit 1
fi
echo "Verified: $(codesign -dv /Applications/SpeechX.app 2>&1 | grep Identifier)"

echo "Done. First launch: right-click SpeechX.app in /Applications -> Open"
echo "(A plain double-click will be blocked by Gatekeeper — expected for an"
echo "un-notarized build, not a bug. Only needed once per rebuild.)"
