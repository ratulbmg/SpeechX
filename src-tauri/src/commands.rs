//! `#[tauri::command]`s for the dashboard window (src/App.tsx) — checking
//! and opening the two permissions it shows: Microphone and Accessibility,
//! the master listening toggle, and quitting the app.
//!
//! Not gated by the capabilities/ACL system in `capabilities/default.json`
//! — that governs *plugin* commands (`core:*`, `opener:*`); commands
//! defined directly in the app crate and registered via
//! `invoke_handler(generate_handler![...])` are callable by any window
//! that can reach the IPC bridge without a separate permission entry.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, State, Wry};

/// Shared with `hotkey::run` (managed Tauri state, constructed once in
/// `lib.rs::run`) — the hotkey loop reads `enabled` to decide whether a
/// Right Command press should arm a session at all. Doesn't touch already
/// in-flight recordings: flipping this off mid-hold lets that one session
/// finish normally, and only blocks the *next* press.
///
/// Listening can be toggled from two places — the dashboard's switch
/// (`set_listening_enabled` below) and the tray menu's checkbox
/// (`tray.rs`) — so `set_listening` is the single point both go through,
/// keeping whichever one the user *didn't* click in sync with the one
/// they did.
pub struct ListeningState {
    pub enabled: Arc<AtomicBool>,
    tray_checkbox: Mutex<Option<CheckMenuItem<Wry>>>,
}

impl ListeningState {
    pub fn new(enabled: Arc<AtomicBool>) -> Self {
        Self {
            enabled,
            tray_checkbox: Mutex::new(None),
        }
    }

    /// Called once, from `tray.rs`, right after the tray menu is built.
    pub fn set_tray_checkbox(&self, item: CheckMenuItem<Wry>) {
        *self.tray_checkbox.lock().unwrap_or_else(|e| e.into_inner()) = Some(item);
    }

    pub fn set_listening(&self, value: bool) {
        self.enabled.store(value, Ordering::Relaxed);
        let guard = self.tray_checkbox.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(item) = guard.as_ref() {
            let _ = item.set_checked(value);
        }
    }
}

/// Read-only — safe to poll on a timer from the dashboard.
#[tauri::command]
pub fn get_listening_enabled(state: State<ListeningState>) -> bool {
    state.enabled.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn set_listening_enabled(enabled: bool, state: State<ListeningState>) {
    state.set_listening(enabled);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Read-only — never prompts. Safe to poll on a timer from the dashboard.
#[tauri::command]
pub fn check_microphone_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::permissions::macos::microphone_authorized()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows has no TCC-style per-app consent for the microphone —
        // just a system-wide privacy toggle `cpal` either can or can't
        // open the device against (see audio/capture.rs's
        // DeviceNotAvailable handling). Nothing to check per-app here.
        true
    }
}

/// Read-only — never prompts. Safe to poll on a timer from the dashboard.
#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::permissions::macos::accessibility_authorized()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // No Accessibility-equivalent consent screen on Windows.
        true
    }
}

/// Read-only — never prompts. Safe to poll on a timer from the dashboard.
///
/// macOS-only: Windows has no equivalent permission gate for global
/// keyboard hooks (see `check_accessibility_permission`'s Windows arm) —
/// `SetWindowsHookEx` just works without OS-level consent, so there's
/// nothing to show a status for there. Always `true` on Windows, matching
/// the pattern the other checks already use, so the dashboard doesn't
/// need its own per-platform branching in `src/App.tsx`.
#[tauri::command]
pub fn check_input_monitoring_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::permissions::macos::input_monitoring_authorized()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Called from the dashboard's Accessibility button — not automatically
/// at launch, so the system prompt only ever appears because the user
/// asked for it.
///
/// Unlike Microphone, Accessibility's system alert reappears every time
/// this is called while access is untrusted, even if the user dismissed
/// it before — its alert has its own "Open System Settings" button, so
/// there's always a path forward without a separate settings-deep-link
/// command here.
#[tauri::command]
pub fn request_accessibility_permission() {
    #[cfg(target_os = "macos")]
    {
        crate::permissions::macos::ensure_accessibility();
    }
}

/// Called from the dashboard's Input Monitoring button — previously
/// bundled into `request_accessibility_permission` (one click requested
/// both, since both gate the same underlying capability: the global
/// hotkey listener), now split out now that the dashboard shows Input
/// Monitoring as its own row with its own status.
///
/// Unlike Accessibility's prompt, `IOHIDRequestAccess`'s system dialog is
/// known to silently fail to appear at all if `AXIsProcessTrustedWithOptions`
/// (Accessibility's check — which the dashboard polls continuously the
/// whole time it's open) has already been called earlier in the same
/// process's life, which by the time anyone reaches this tab it always
/// has. So this can't be relied on alone: also deep-link straight to
/// System Settings' Input Monitoring pane, same pattern as the old
/// settings-opening commands this file used to have before Accessibility
/// switched to prompting directly — a manual path that works regardless
/// of whether the native dialog rendered.
#[tauri::command]
pub fn request_input_monitoring_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::permissions::macos::ensure_input_monitoring();
        tauri_plugin_opener::open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            None::<&str>,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Called from the dashboard's Microphone button. Blocking (waits on
/// CoreAudio and, the first time, the user's answer to the system
/// dialog) — fine here since Tauri runs command handlers off the main
/// thread already.
///
/// Note: unlike Accessibility, macOS's microphone consent is a genuine
/// one-time decision — if the user has already denied it, this call
/// fails silently with no dialog (same as any other retry), and the only
/// way back is System Settings. Not handled here; the dashboard doesn't
/// yet distinguish "not yet decided" from "already denied" for mic.
#[tauri::command]
pub fn request_microphone_permission() {
    #[cfg(target_os = "macos")]
    {
        crate::audio::warm_up_microphone_permission();
    }
}
