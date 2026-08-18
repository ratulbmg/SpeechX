
//! Windows pill overlay — transparent, always-on-top, non-activating
//! floating window that shows recording / transcribing state.
//!
//! Mirrors the macOS NSPanel approach in `panel.rs` but uses a plain Tauri
//! `WebviewWindowBuilder` window with Win32 extended-style flags applied
//! immediately after creation to prevent focus theft. Shown with
//! `SW_SHOWNOACTIVATE` so the user's active application keeps focus and
//! text injection lands in the right place.
//!
//! Win32 functions are declared via `extern "system"` (not via the `windows`
//! crate) to avoid crate-version conflicts: `tauri`'s internal windows
//! dependency may resolve to a different version than our `[dependencies]`
//! entry, making their `HWND` types mutually incompatible at the trait level.
//! Since `HWND` is always `#[repr(transparent)]` around `*mut c_void` in all
//! windows crate versions, we transmute the value returned by `hwnd()` to a
//! plain raw pointer and pass it to our own extern declarations.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Position, WebviewUrl, WebviewWindowBuilder};

use super::position::pill_position;

// SW_HIDE = 0, SW_SHOWNOACTIVATE = 4 (MSDN SHOW_WINDOW_CMD)
const SW_HIDE: i32 = 0;
const SW_SHOWNOACTIVATE: i32 = 4;

const PILL_LABEL: &str = "pill";
const PILL_WIDTH: f64 = 180.0;
const PILL_HEIGHT: f64 = 48.0;
const MATCH_FLASH_MS: u64 = 1200;

#[link(name = "user32")]
extern "system" {
    fn ShowWindow(hwnd: *mut std::ffi::c_void, n_cmd_show: i32) -> i32;
}

/// Extract the raw Win32 HWND pointer from a Tauri window.
///
/// `HWND` is `#[repr(transparent)]` over `*mut c_void` in all versions of
/// the windows crate, so transmuting is sound — the in-memory representation
/// is identical regardless of which crate version `hwnd()` used internally.
fn hwnd_raw(window: &tauri::WebviewWindow) -> Option<*mut std::ffi::c_void> {
    let h = window.hwnd().ok()?;
    // SAFETY: HWND is always a transparent newtype over *mut c_void.
    Some(unsafe { std::mem::transmute(h) })
}

/// Create the pill window at app startup — hidden, borderless, transparent,
/// always-on-top, never appearing in Alt+Tab. Returns immediately; the window
/// is not shown until `emit_show` is called.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(
        app,
        PILL_LABEL,
        WebviewUrl::App("src/pill.html".into()),
    )
    .title("SpeechX Overlay")
    .inner_size(PILL_WIDTH, PILL_HEIGHT)
    .decorations(false)
    .transparent(true)
    // Explicitly zero the WebView2 controller's background colour so the
    // area outside the pill's CSS border-radius is truly transparent rather
    // than the default white/dark rectangle that WebView2 paints by default
    // even when the OS window itself is transparent.
    .background_color(tauri::window::Color(0, 0, 0, 0))
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;

    // Apply WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST |
    // WS_EX_LAYERED so it can never steal focus or show in Alt+Tab.
    if let Some(raw) = hwnd_raw(&window) {
        super::windows::make_non_activating(raw);
    }

    Ok(())
}

fn reposition(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PILL_LABEL) else {
        return;
    };
    let (x, y) = match window.primary_monitor() {
        Ok(Some(m)) => pill_position((m.size().width, m.size().height), m.scale_factor()),
        _ => pill_position((1920, 1080), 1.0),
    };
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

fn show_window(app: &AppHandle) {
    let owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        reposition(&owned);
        if let Some(window) = owned.get_webview_window(PILL_LABEL) {
            if let Some(raw) = hwnd_raw(&window) {
                unsafe { ShowWindow(raw, SW_SHOWNOACTIVATE) };
            }
        }
    });
}

fn hide_window(app: &AppHandle) {
    let owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = owned.get_webview_window(PILL_LABEL) {
            if let Some(raw) = hwnd_raw(&window) {
                unsafe { ShowWindow(raw, SW_HIDE) };
            }
        }
    });
}

fn hide_after(app: AppHandle, delay: Duration) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        let _ = app.emit_to(PILL_LABEL, "pill-hide", ());
        hide_window(&app);
    });
}

#[derive(Serialize, Clone)]
struct ShowPayload {
    mode: String,
    language: String,
}

pub fn emit_show(app: &AppHandle, mode: crate::hotkey::chord::Mode, language: &str) {
    use crate::hotkey::chord::Mode;
    let mode_str = match mode {
        Mode::Dictate => "dictate",
        Mode::Command => "command",
    };
    show_window(app);
    let _ = app.emit_to(
        PILL_LABEL,
        "pill-show",
        ShowPayload {
            mode: mode_str.to_string(),
            language: language.to_string(),
        },
    );
}

pub fn emit_transcribing(app: &AppHandle) {
    let _ = app.emit_to(PILL_LABEL, "pill-transcribing", ());
}

pub fn emit_hide(app: &AppHandle) {
    let _ = app.emit_to(PILL_LABEL, "pill-hide", ());
    hide_window(app);
}

pub fn emit_cancelled(app: &AppHandle) {
    let _ = app.emit_to(PILL_LABEL, "pill-cancelled", ());
    hide_window(app);
}

pub fn emit_matched(app: &AppHandle, language: &str) {
    let _ = app.emit_to(PILL_LABEL, "pill-matched", language.to_string());
    hide_after(app.clone(), Duration::from_millis(MATCH_FLASH_MS));
}

pub fn emit_audio_level(app: &AppHandle, level: f32) {
    let _ = app.emit_to(PILL_LABEL, "audio-level", level);
}
