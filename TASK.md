# Windows Test Checklist — Microphone Selection + Permission Onboarding

Context: these changes were made and verified on macOS only (branch
`feature/microphone-selection`, all currently uncommitted). Windows-target
code was only checked with `cargo check --target x86_64-pc-windows-msvc`
(a cross-compile from the Mac — it catches type errors but was never
actually **run**). Nothing below has been executed on real Windows
hardware yet.

## Exactly what changed, file by file

### `src-tauri/src/audio/capture.rs` — cross-platform, needs testing only
- Added `list_input_device_names()` — enumerates input devices via `cpal`.
- Added `resolve_device()` — resolves a selected device by name, falling
  back to system default if unset or no longer present (e.g. unplugged).
- `CaptureSession::start()` now takes `device_name: Option<&str>` instead
  of always using the default device.
- No `#[cfg(target_os = ...)]` anywhere in this file — `cpal` abstracts
  CoreAudio vs WASAPI itself, so this is the same code path on both
  platforms. **Nothing Windows-specific needs to be written here** — it
  just needs to be run and confirmed working.

### `src-tauri/src/audio/mod.rs` / `src-tauri/src/hotkey/mod.rs` — cross-platform, needs testing only
- `AudioController` now holds `selected_microphone: Arc<Mutex<Option<String>>>`
  and reads it fresh on every `start()` call.
- `hotkey::run()` takes an extra `selected_microphone` parameter, threaded
  through from `lib.rs`.
- Also no platform-specific code. Same story as above — just needs a real run.

### `src-tauri/src/commands.rs` — cross-platform additions, plus a removal
- Added: `SelectedMicrophone` state, `list_microphones`, `get_selected_microphone`,
  `set_selected_microphone` — all plain, no `cfg` gating, work identically
  on both platforms.
- **Removed**: `request_accessibility_permission`, `request_input_monitoring_permission`,
  `request_microphone_permission` — these used to be called by the
  dashboard's now-deleted "Grant Access" buttons. On Windows these were
  already almost no-ops (their bodies were `#[cfg(target_os = "macos")]`
  gated internally), so removing them changes nothing functionally for
  Windows — but confirm the frontend doesn't still try to `invoke()` any
  of these three (it shouldn't — see `App.tsx` changes below).

### `src-tauri/src/lib.rs` — this is the one with an actual platform split
- New `onboard_permissions_sequentially()` — **macOS-only**
  (`#[cfg(target_os = "macos")]`). Requests Accessibility, waits for it to
  resolve, then Input Monitoring, waits, then microphone. There is
  deliberately **no Windows equivalent of this function** — Windows has
  no Accessibility/Input Monitoring concept to sequence through.
- For Windows, the launch path is instead:
  ```rust
  #[cfg(not(target_os = "macos"))]
  tauri::async_runtime::spawn_blocking(audio::warm_up_microphone_permission);
  ```
  i.e. just fire off the mic warm-up directly, no sequencing needed since
  there's only one prompt to trigger.
- **This Windows branch has never been run.** It's the main thing in this
  whole changeset that's actually new Windows-path code rather than
  shared/cross-platform code — it needs to be confirmed it (a) compiles
  in a real Windows build (already cross-checked, but not built for
  real), and (b) actually triggers Windows' microphone consent behavior
  at launch without hanging or crashing.

### `src/App.tsx` / `src/App.css` — shared UI, platform-conditional rendering
- `PermissionRow` no longer takes `onRequest`/`locked`/`lockedLabel` props
  — it's just a label + a colored status span now (`.permission-status`,
  green "Granted" / gray "Not granted"). No button anywhere.
- The Accessibility and Input Monitoring rows are wrapped in `!IS_WINDOWS`
  (unchanged from before — this predates this session's changes). Only
  the Microphone row is unconditional, so **Windows' Permissions tab
  should show exactly one row**.
- Added the `MicrophoneSelector` dropdown to the Controls tab —
  unconditional, shows on both platforms.

## What's genuinely untested vs. what's a real gap

- **Untested but should just work** (cpal-abstracted, no platform
  branching): microphone enumeration, device selection, dictation from a
  non-default device. Just needs a run to confirm.
- **New Windows-specific code path, never executed**: the
  `spawn_blocking(audio::warm_up_microphone_permission)` launch-time call
  in `lib.rs`. This is the one piece of logic written for Windows this
  session that has zero real-world verification.
- **Known design gap, not new to this session**: `check_microphone_permission`
  (`commands.rs`) always returns `true` on Windows — there's no per-app
  consent API to query there, only the system-wide privacy toggle. This
  means the Permissions tab's Microphone row can show "Granted" even when
  the Windows privacy toggle is off and dictation can't actually capture
  audio. This was already true before this session's changes; it's just
  more visible now that the row is a plain status badge instead of a
  clickable "Grant Access" button. Worth deciding whether it needs a real
  fix (e.g. attempting a device open and checking whether it actually
  yields audio) or is acceptable as-is.

## Build

- [ ] `npm run tauri build` completes cleanly on Windows (real MSVC
      toolchain, not just the cross-compile check).
- [ ] Resulting installer (MSI/NSIS) installs and launches without errors.

## Microphone selection

- [ ] Controls tab shows a "Microphone" dropdown populated with actual
      input devices (built-in mic, any USB/Bluetooth headset connected).
- [ ] Selecting a non-default device and dictating (hold Right Alt)
      actually records from that device — easiest way to confirm: pick a
      headset mic, mute/disconnect the laptop's built-in mic, and check
      dictation still works.
- [ ] Switching back to "System Default" works and tracks whatever
      Windows currently considers the default input device.
- [ ] Device list refreshes reasonably if a device is plugged in/removed
      while the dashboard is open (it polls every ~2s).

## Permission onboarding / Permissions tab

- [ ] On a **fresh install** (or after clearing the app's mic permission
      via Settings → Privacy & security → Microphone), launching SpeechX
      triggers Windows' own microphone consent behavior automatically, at
      launch, without the user doing anything in the dashboard first.
- [ ] Permissions tab shows **only** the Microphone row — confirm
      Accessibility/Input Monitoring are actually absent, not just empty.
- [ ] Microphone row has no button — just a colored "Granted"/"Not
      granted" block.
- [ ] With Windows' microphone privacy toggle turned **off**
      (Settings → Privacy & security → Microphone → "Let desktop apps
      access your microphone" disabled), confirm SpeechX doesn't crash or
      hang at launch. Then check whether the Microphone row still shows
      "Granted" despite dictation not being able to capture audio (see
      the known gap noted above) — report back either way.

## Regression check (should be unaffected, but confirm)

- [ ] Hold Right Alt still arms/starts dictation as before.
- [ ] Listening toggle (Controls tab) still works.
- [ ] Tray icon → "Open Dashboard" and quitting/reopening the window both
      still work.
- [ ] Dashboard window isn't oddly laid out now that the Permissions tab
      only has one row on Windows (e.g. excess empty space, squished Quit
      button) — window height is a single fixed value shared with macOS.
