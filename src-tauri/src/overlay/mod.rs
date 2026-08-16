//! The pill overlay window (PROMPT.md §10 / M5) hasn't been built yet.
//! This module currently only holds the Windows non-activating-window
//! mechanism (WINDOWS_SUPPORT.md §4) so it exists and type-checks ahead
//! of M5; the macOS `tauri-nspanel` equivalent lands with the pill itself.

pub mod position;

#[cfg(target_os = "windows")]
pub mod windows;
