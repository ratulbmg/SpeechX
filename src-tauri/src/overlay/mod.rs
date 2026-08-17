//! The pill overlay window (PROMPT.md §10 / M5).
//!
//! `panel` holds the macOS `tauri-nspanel` implementation. The Windows
//! non-activating-window mechanism (WINDOWS_SUPPORT.md §4) is still just
//! a stub in `windows` — wiring it up to real session events is a
//! follow-up, not part of this pass.

pub mod position;

#[cfg(target_os = "macos")]
pub mod panel;

#[cfg(target_os = "windows")]
pub mod windows;
