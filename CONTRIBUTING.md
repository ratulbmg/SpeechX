# Contributing to SpeechX

This covers building SpeechX from source and finding your way around the codebase. For
what the app does and how to install a release build, see [`README.md`](./README.md).

## Development setup

```bash
npm install
npm run tauri dev
```

Requires Rust (stable) and Node 18+.

On macOS, permissions are requested on demand rather than automatically at launch —
Accessibility, Input Monitoring, and Microphone access all get requested from the
dashboard's **Permissions** tab, not on startup. If the hotkey listener doesn't seem to
react to keypresses, that's almost always a missing permission grant, not a crash: check
the Permissions tab first.

### ASR models

The bundled speech models are gitignored (large binaries) and aren't part of a fresh
checkout. Fetch them before building:

```bash
./scripts/download_models.sh
```

`tauri build` (and even `cargo check`/`cargo clippy`, since Tauri's build script
validates every bundled resource path) will fail with a missing-resource error until
this has been run once.

## Layout

- `src-tauri/` — Rust backend.
  - `hotkey/` — the global key listener and the chord state machine that decides
    Dictate vs. Command mode from Right-side modifier combinations.
  - `audio/` — microphone capture.
  - `asr/` — the Whisper and streaming-Zipformer engines (via `sherpa-onnx`), and the
    manager that lazily loads/caches them per language.
  - `inject/` — typing the transcribed text into whatever app has focus.
  - `overlay/` — the small recording-status pill window.
  - `permissions/` — macOS Accessibility/Input Monitoring/Microphone permission checks
    and on-demand prompts.
  - `tray.rs` — the menu bar icon and its dropdown (Open Dashboard / Listening toggle /
    Quit), the primary way back into the dashboard since the app has no Dock icon.
  - `commands.rs` — the `#[tauri::command]`s the dashboard window calls over IPC.
  - `lang/` — the language registry (`languages.toml`) and the fuzzy matcher used for
    voice-driven language switching.
- `src/` — React/TS frontend: the dashboard window (`App.tsx`) and the recording pill
  overlay (`pill.html`/`src/pill.*`).
- `scripts/` — build and release helpers:
  - `download_models.sh` — fetches the ASR models (see above).
  - `install_local.sh` — builds from source and installs to `/Applications` for local
    testing, handling the ad-hoc re-signing and quarantine flag automatically.
  - `install_latest_release.sh` — downloads and installs the latest published GitHub
    Release without needing a source checkout at all; also runnable directly via
    `curl | bash` on a machine with nothing cloned.
  - `release.sh` — builds, signs, and notarizes the distributable `.dmg` (requires a
    paid Apple Developer account; not used for day-to-day development).
- `.github/workflows/`
  - `ci.yml` — runs on every PR into `main`/`develop`: type-checks, lints, and builds
    both platforms. Skips docs-only (`.md`) changes.
  - `release.yml` — runs only when a `vX.Y.Z` tag is pushed: builds the universal macOS
    `.dmg` and Windows installers, and publishes a GitHub Release.

## Folder Architecture

```
SpeechX/
├── src/                        React/TS frontend
│   ├── App.tsx                 Dashboard window (Controls / Permissions tabs)
│   ├── App.css
│   ├── pill.tsx, pill.html,    Recording-status pill overlay window
│   │   pill.css
│   ├── components/
│   │   └── SonicWaveform.tsx   Pill's live audio-level visualization
│   └── main.tsx                Vite/React entry point
│
├── src-tauri/                  Rust backend (Tauri)
│   ├── src/
│   │   ├── lib.rs              App setup: window, tray, hotkey loop, model preload
│   │   ├── main.rs             Binary entry point (calls lib.rs's run())
│   │   ├── commands.rs         #[tauri::command]s the dashboard calls over IPC
│   │   ├── tray.rs             Menu bar icon + dropdown menu
│   │   ├── hotkey/             Global key listener + Dictate/Command chord state machine
│   │   ├── audio/               Microphone capture
│   │   ├── asr/                 Whisper + streaming-Zipformer engines, per-language routing
│   │   ├── inject/              Types transcribed text into the focused app
│   │   ├── overlay/             Pill window show/hide/position logic
│   │   ├── permissions/         macOS Accessibility/Input Monitoring/Microphone checks
│   │   ├── lang/                Language registry (languages.toml) + fuzzy matcher
│   │   └── config/              Settings + on-disk paths (models dir, etc.)
│   ├── icons/                  App icons + the tray icon
│   ├── capabilities/           Tauri ACL — plugin command permissions
│   ├── Info.plist              macOS usage-description strings, merged into the bundle
│   ├── Entitlements.plist      macOS entitlements (mic, hardened-runtime exceptions)
│   ├── tauri.conf.json         App metadata, window config, bundle settings
│   └── Cargo.toml
│
├── models/                     ASR model weights (gitignored — see download_models.sh)
│
├── scripts/                    Build/install/release helpers (see Layout above)
│
└── .github/workflows/
    ├── ci.yml                  Per-PR: type-check, lint, build both platforms
    └── release.yml             Per-tag: build + publish a GitHub Release
```

## Releasing

A release is triggered by pushing a version tag, not by merging a PR:

```bash
git tag v1.4.0
git push origin v1.4.0
```

Before tagging, make sure the version has been bumped consistently in all three places
that declare it — `package.json`, `src-tauri/tauri.conf.json`, and
`src-tauri/Cargo.toml` — and that the bump is actually **committed** (an uncommitted
version change won't be part of whatever commit the tag points to).
