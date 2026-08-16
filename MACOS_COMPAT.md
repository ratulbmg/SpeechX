# SpeechX — macOS Universal Build & Compatibility

> **How to use:** Save as `MACOS_COMPAT.md` in your `speech_x/` folder.
> Tell Claude Code: *"Read MACOS_COMPAT.md and implement it fully."*
> This prompt assumes the core app is already built. This prompt only handles
> making it run on every Mac — M1 through M5 and Intel.

---

## Goal

Produce a single `.app` and `.dmg` that installs and runs correctly on:

| Chip | macOS minimum |
|---|---|
| Intel (x86_64) | macOS 11 Big Sur |
| Apple Silicon M1 | macOS 11 Big Sur |
| Apple Silicon M2 | macOS 12 Monterey |
| Apple Silicon M3 | macOS 13 Ventura |
| Apple Silicon M4 | macOS 14 Sonoma |
| Apple Silicon M5 | macOS 15 Sequoia |

One binary. One installer. No separate Intel vs Apple Silicon downloads.
This is called a **Universal Binary** (also called "fat binary") — it contains
both the x86_64 and aarch64 builds stitched together. macOS automatically runs
the right slice for the current chip.

---

## Step 1 — Add both Rust targets

```bash
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
```

Verify both are installed:

```bash
rustup target list --installed | grep apple
# Should show:
# aarch64-apple-darwin
# x86_64-apple-darwin
```

---

## Step 2 — Set the minimum macOS version

Every place a minimum version is set must agree. If they disagree, the build
either fails or produces an app that crashes on older Macs silently.

### 2a. `src-tauri/tauri.conf.json`

```json
{
  "bundle": {
    "macOS": {
      "minimumSystemVersion": "11.0"
    }
  }
}
```

### 2b. `src-tauri/Cargo.toml`

Add this section if it does not exist:

```toml
[package.metadata.bundle]
osx_minimum_system_version = "11.0"
```

### 2c. Environment variable during build

Set `MACOSX_DEPLOYMENT_TARGET=11.0` in every build command and in CI.
This tells the Rust linker what the minimum target is and prevents it from
linking symbols that only exist in newer macOS versions.

---

## Step 3 — sherpa-onnx universal build

sherpa-onnx ships pre-built static libraries. You need both architectures.

In `src-tauri/build.rs`, detect the target and link the correct library:

```rust
fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    match target.as_str() {
        "aarch64" => {
            println!("cargo:rustc-link-search=native={}/libs/aarch64", manifest_dir);
        }
        "x86_64" => {
            println!("cargo:rustc-link-search=native={}/libs/x86_64", manifest_dir);
        }
        _ => panic!("Unsupported architecture: {}", target),
    }

    println!("cargo:rustc-link-lib=static=sherpa-onnx");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreAudio");
    println!("cargo:rustc-link-lib=framework=AudioToolbox");
    println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=11.0");
}
```

Place the pre-built libraries at:

```
src-tauri/
└── libs/
    ├── aarch64/
    │   └── libsherpa-onnx.a
    └── x86_64/
        └── libsherpa-onnx.a
```

Download script for `scripts/download_sherpa_libs.sh`:

```bash
#!/bin/bash
set -e

SHERPA_VERSION="1.10.30"   # update to latest stable
BASE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}"

mkdir -p src-tauri/libs/aarch64
mkdir -p src-tauri/libs/x86_64

# Apple Silicon
curl -L "${BASE_URL}/sherpa-onnx-v${SHERPA_VERSION}-osx-arm64-static.tar.bz2" \
  | tar -xj -C src-tauri/libs/aarch64 --strip-components=2 "*/lib/libsherpa-onnx-c-api.a"
mv src-tauri/libs/aarch64/libsherpa-onnx-c-api.a src-tauri/libs/aarch64/libsherpa-onnx.a

# Intel
curl -L "${BASE_URL}/sherpa-onnx-v${SHERPA_VERSION}-osx-x86_64-static.tar.bz2" \
  | tar -xj -C src-tauri/libs/x86_64 --strip-components=2 "*/lib/libsherpa-onnx-c-api.a"
mv src-tauri/libs/x86_64/libsherpa-onnx-c-api.a src-tauri/libs/x86_64/libsherpa-onnx.a

echo "Done. Verify:"
file src-tauri/libs/aarch64/libsherpa-onnx.a
file src-tauri/libs/x86_64/libsherpa-onnx.a
```

Run once before your first universal build:

```bash
chmod +x scripts/download_sherpa_libs.sh
./scripts/download_sherpa_libs.sh
```

---

## Step 4 — ASR models work on all chips unchanged

The ASR models (Whisper, IndicConformer) are ONNX format. ONNX Runtime
inside sherpa-onnx handles CPU inference identically on Intel and Apple Silicon.
No model changes needed. The same `.onnx` files downloaded by the user work on
every Mac.

However, on Apple Silicon, ONNX Runtime can optionally use the **ANE (Apple
Neural Engine)** via CoreML for faster inference. Add this as an optional
performance setting, not the default — CoreML adds complexity and the CPU path
is already fast enough for dictation:

```toml
# settings.toml
[performance]
use_coreml = false   # set to true on M-series for ~40% faster transcription
```

In `asr/manager.rs`, pass the CoreML execution provider when the setting is on
and the chip is Apple Silicon:

```rust
#[cfg(target_arch = "aarch64")]
if settings.performance.use_coreml {
    providers.push(ExecutionProvider::CoreML(CoreMLExecutionProviderOptions {
        use_cpu_only: false,
        enable_on_subgraph: false,
        only_enable_device_with_ane: true,
    }));
}
```

Never enable CoreML on Intel — it does not exist there and will crash.

---

## Step 5 — `cpal` audio on Intel vs Apple Silicon

`cpal` works identically on both. No changes needed in audio capture code.

One thing to verify: on Intel Macs, the default audio device sample rate is
often **44100 Hz** rather than 48000 Hz. Your resample pipeline already handles
any input rate → 16 kHz mono, so this is fine. Confirm `audio/resample.rs`
reads the actual device sample rate from `cpal` rather than assuming 48000.

```rust
// audio/capture.rs — do this, not hardcoding 48000
let config = device.default_input_config()?;
let device_sample_rate = config.sample_rate().0;  // whatever the device reports
// pass device_sample_rate into resample.rs
```

---

## Step 6 — Build the Universal Binary

### Development (single architecture, fast)

For daily development, build only for your own chip — much faster compile:

```bash
# You're on M4/M5 (Apple Silicon), so this is your fast dev build:
npm run tauri dev
# or
npm run tauri build -- --target aarch64-apple-darwin
```

### Universal release build

Only run this when you want to distribute or test on Intel:

```bash
MACOSX_DEPLOYMENT_TARGET=11.0 npm run tauri build -- --target universal-apple-darwin
```

Output:

```
src-tauri/target/universal-apple-darwin/release/bundle/
├── macos/
│   └── SpeechX.app          ← contains BOTH arm64 and x86_64 slices
└── dmg/
    └── SpeechX_x.x.x_universal.dmg
```

Verify the binary contains both architectures:

```bash
lipo -info "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app/Contents/MacOS/SpeechX"
# Must output:
# Architectures in the fat file: ... are: x86_64 arm64
```

If it shows only one architecture, the build configuration is wrong.

---

## Step 7 — Signing for distribution

Unsigned universal binaries trigger Gatekeeper on every machine except yours.
Users right-click and get blocked. Accessibility permissions misbehave.

### For your own machine only (self-signed)

```bash
# Create a self-signed certificate once in Keychain Access:
# Keychain Access → Certificate Assistant → Create a Certificate
# Name: SpeechX Dev | Type: Code Signing | Self Signed Root

# Then sign:
codesign --force --deep --sign "SpeechX Dev" \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app"
```

### For distributing to other people (Apple Developer account required)

```bash
# Replace "Developer ID Application: Your Name (TEAMID)" with your actual cert
CERT="Developer ID Application: Your Name (TEAMID)"
APP="src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app"

# Sign with hardened runtime (required for notarization)
codesign --force --deep --sign "$CERT" \
  --options runtime \
  --entitlements src-tauri/Entitlements.plist \
  "$APP"

# Notarize (Apple scans it, takes 2–10 minutes)
xcrun notarytool submit "${APP%/*}/../dmg/SpeechX_universal.dmg" \
  --apple-id "your@email.com" \
  --team-id "TEAMID" \
  --password "app-specific-password" \
  --wait

# Staple the notarization ticket to the dmg
xcrun stapler staple "${APP%/*}/../dmg/SpeechX_universal.dmg"
```

Save this as `scripts/release.sh`. Run it only when publishing a release,
not during development.

---

## Step 8 — Entitlements

The `src-tauri/Entitlements.plist` must include these for the app to work
correctly on all chips after signing:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <!-- Microphone access -->
    <key>com.apple.security.device.audio-input</key>
    <true/>

    <!-- Required for hardened runtime to allow JIT / dynamic code.
         sherpa-onnx needs this. -->
    <key>com.apple.security.cs.allow-jit</key>
    <true/>

    <!-- Required for sherpa-onnx ONNX Runtime on hardened runtime -->
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
    <true/>

    <!-- Allows loading the ONNX model files at runtime -->
    <key>com.apple.security.cs.disable-library-validation</key>
    <true/>
</dict>
</plist>
```

Without `allow-unsigned-executable-memory`, ONNX Runtime crashes on hardened
runtime builds. This affects both Intel and Apple Silicon equally.

---

## Step 9 — GitHub Actions for automatic builds (optional but recommended)

When you are ready to release to others, this workflow builds a universal `.dmg`
automatically every time you push a git tag.

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build-macos:
    runs-on: macos-15        # Sequoia runner, has Xcode 16
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin

      - name: Install Node
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'

      - name: Download sherpa-onnx libraries
        run: ./scripts/download_sherpa_libs.sh

      - name: Install npm dependencies
        run: npm ci

      - name: Build universal binary
        env:
          MACOSX_DEPLOYMENT_TARGET: '11.0'
          # For signed builds, add these secrets in GitHub repo settings:
          # APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD,
          # APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_TEAM_ID,
          # APPLE_APP_SPECIFIC_PASSWORD
        run: |
          npm run tauri build -- --target universal-apple-darwin

      - name: Verify universal binary
        run: |
          lipo -info \
            "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app/Contents/MacOS/SpeechX"

      - name: Upload DMG
        uses: actions/upload-artifact@v4
        with:
          name: SpeechX-universal-dmg
          path: src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg
```

To trigger a release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub builds the universal `.dmg` and attaches it to the release automatically.
Anyone on any Mac downloads one file and it works.

---

## Step 10 — Testing checklist before any release

Run through this on your own M-series Mac before publishing:

```bash
# 1. Verify the binary is truly universal
lipo -info "...SpeechX.app/Contents/MacOS/SpeechX"
# Must show: x86_64 arm64

# 2. Run the x86_64 slice on your M-series Mac using Rosetta
# (Tests Intel compatibility without needing an Intel machine)
arch -x86_64 \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app/Contents/MacOS/SpeechX"
# Should launch and function normally

# 3. Check minimum OS version embedded in the binary
vtool -show-build \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app/Contents/MacOS/SpeechX"
# minos should be 11.0 for both slices

# 4. Check for any symbols that require newer macOS than 11.0
# (catches accidental use of APIs that don't exist on Big Sur)
nm -u "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app/Contents/MacOS/SpeechX" \
  | grep "OBJC\|NS\|CF" | head -30
# Review — if anything looks like a macOS 12+ API, investigate

# 5. Verify signing
codesign --verify --deep --strict \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app"
codesign -dv --verbose=4 \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app"

# 6. Verify Gatekeeper would accept it (for notarized builds)
spctl --assess --type execute \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app"
# Should output: accepted
```

---

## Quick reference — commands you will actually type

```bash
# One-time setup
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/download_sherpa_libs.sh

# Daily development (fast, your chip only)
npm run tauri dev

# Test on your machine as a real app
npm run tauri build -- --target aarch64-apple-darwin

# Universal build for distribution (slow, ~10 min)
MACOSX_DEPLOYMENT_TARGET=11.0 npm run tauri build -- --target universal-apple-darwin

# Verify it's truly universal
lipo -info "src-tauri/target/universal-apple-darwin/release/bundle/macos/SpeechX.app/Contents/MacOS/SpeechX"

# Test Intel slice on your Apple Silicon Mac
arch -x86_64 "...SpeechX.app/Contents/MacOS/SpeechX"

# Release (tag triggers GitHub Actions to build and publish)
git tag v0.1.0 && git push origin v0.1.0
```

---

## What M1 through M5 means in practice

All Apple Silicon chips — M1, M2, M3, M4, M5 — run the same `aarch64-apple-darwin`
binary. They are the same CPU architecture. There is no M1-specific binary vs
M5-specific binary. One arm64 build runs on all of them.

The only split is **arm64 vs x86_64**. That is the entire reason for the
Universal Binary. One binary, two slices, every Mac.

| Chip | Architecture | Binary slice used |
|---|---|---|
| Intel Core i5/i7/i9 | x86_64 | x86_64 slice |
| M1 / M1 Pro / M1 Max / M1 Ultra | arm64 | arm64 slice |
| M2 / M2 Pro / M2 Max / M2 Ultra | arm64 | arm64 slice |
| M3 / M3 Pro / M3 Max | arm64 | arm64 slice |
| M4 / M4 Pro / M4 Max | arm64 | arm64 slice |
| M5 / M5 Pro / M5 Max | arm64 | arm64 slice |

Intel Macs can also run the arm64 slice through **Rosetta 2**, but shipping
the universal binary means they run native x86_64 instead — better performance
and no Rosetta overhead.
