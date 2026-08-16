//! Types text at the current cursor position.
//!
//! macOS: a synthetic Unicode keyboard event via `enigo` — the same
//! mechanism as PROMPT.md §9's `CGEventKeyboardSetUnicodeString`
//! strategy. `enigo`'s macOS backend posts Unicode directly, so
//! Devanagari/Bengali work with no clipboard round-trip. The full §9
//! fallback chain (AXUIElement first, then this, then clipboard+Cmd+V
//! as a last resort) and the undo buffer are not implemented yet.
//!
//! Windows: UI Automation's `ValuePattern` first (whole-string insert,
//! more reliable in UWP/WinUI apps), falling back to `SendInput` with
//! `KEYEVENTF_UNICODE` (WINDOWS_SUPPORT.md §5–§6) when the focused
//! element doesn't support it.

#[cfg(target_os = "windows")]
mod windows_sendinput;
#[cfg(target_os = "windows")]
mod windows_uia;

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    // Only constructed on macOS (enigo's connection step) — Windows'
    // strategies don't have an equivalent "connect" phase.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    #[error("failed to connect to the input system: {0}")]
    Connect(String),
    #[error("failed to send text: {0}")]
    Send(String),
}

#[cfg(target_os = "macos")]
pub fn inject_text(text: &str) -> Result<(), InjectError> {
    use enigo::{Enigo, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| InjectError::Connect(e.to_string()))?;
    enigo.text(text).map_err(|e| InjectError::Send(e.to_string()))?;
    tracing::info!(chars = text.chars().count(), "text injected");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn inject_text(text: &str) -> Result<(), InjectError> {
    if windows_uia::inject_text(text).is_ok() {
        tracing::info!(chars = text.chars().count(), strategy = "uia", "text injected");
        return Ok(());
    }

    windows_sendinput::inject_text(text).map_err(InjectError::Send)?;
    tracing::info!(chars = text.chars().count(), strategy = "send_input", "text injected");
    Ok(())
}
