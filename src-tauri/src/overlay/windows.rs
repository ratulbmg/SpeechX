
//! Windows equivalent of macOS's `tauri-nspanel` non-activating panel
//! (WINDOWS_SUPPORT.md §4): extended window style flags that keep the
//! pill from stealing keyboard focus when shown.
//!
//! Win32 functions are declared directly via `extern "system"` so this
//! module remains decoupled from any specific version of the `windows` crate
//! — `tauri` and our own `[dependencies.windows]` may resolve to different
//! versions, and mixing their `HWND` types causes trait-bound failures.

// Win32 extended-style constants
const GWL_EXSTYLE: i32 = -20;
const WS_EX_NOACTIVATE: i32 = 0x0800_0000_u32 as i32;
const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;
const WS_EX_TOPMOST: i32 = 0x0000_0008;
const WS_EX_LAYERED: i32 = 0x0008_0000;

#[link(name = "user32")]
extern "system" {
    fn GetWindowLongW(hwnd: *mut std::ffi::c_void, n_index: i32) -> i32;
    fn SetWindowLongW(hwnd: *mut std::ffi::c_void, n_index: i32, dw_new_long: i32) -> i32;
}

/// Call immediately after the pill window is created. Sets flags so the
/// window never steals focus, never appears in Alt+Tab, always stays on
/// top, and supports per-pixel transparency.
pub fn make_non_activating(hwnd: *mut std::ffi::c_void) {
    // SAFETY: `hwnd` must be a valid window handle for the lifetime of
    // this call, which the caller guarantees by passing a window it just
    // created and still owns.
    unsafe {
        let current = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_style =
            current | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED;
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_style);
    }
}
