//! Windows equivalent of macOS's `tauri-nspanel` non-activating panel
//! (WINDOWS_SUPPORT.md §4): extended window style flags that keep the
//! pill from stealing keyboard focus when shown.
//!
//! Not wired to a live window yet — the pill overlay itself (PROMPT.md
//! M5) hasn't been built. This is here so the Windows-specific mechanism
//! exists and type-checks; call `make_non_activating` right after the
//! pill's `HWND` is created once M5 lands.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST,
};

/// Call immediately after the pill window is created. Sets flags so the
/// window never steals focus, never appears in Alt+Tab, always stays on
/// top, and supports transparency.
#[allow(dead_code)] // no caller until the pill (M5) creates a window to call this on
pub fn make_non_activating(hwnd: HWND) {
    // SAFETY: `hwnd` must be a valid window handle for the lifetime of
    // this call, which the caller guarantees by passing a window it just
    // created and still owns.
    unsafe {
        let current = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_style = current
            | WS_EX_NOACTIVATE.0 as i32
            | WS_EX_TOOLWINDOW.0 as i32
            | WS_EX_TOPMOST.0 as i32
            | WS_EX_LAYERED.0 as i32;
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_style);
    }
}
