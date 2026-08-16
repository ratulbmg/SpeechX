//! Windows text injection via UI Automation's `ValuePattern` — inserts the
//! whole string in one call instead of character-by-character `SendInput`,
//! and works more reliably in UWP/WinUI apps (WINDOWS_SUPPORT.md §6).
//! Falls back to `windows_sendinput` when the focused element doesn't
//! support `ValuePattern` (most native text fields do; some custom
//! controls don't).

use windows::core::Interface;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern, UIA_ValuePatternId,
};

pub fn inject_text(text: &str) -> Result<(), String> {
    unsafe {
        // Ignore the result: this may already be initialized on this
        // thread (e.g. by Tauri's WebView2 host), which is not an error.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("failed to create UI Automation instance: {e}"))?;

        let focused: IUIAutomationElement = automation
            .GetFocusedElement()
            .map_err(|e| format!("GetFocusedElement failed: {e}"))?;

        let pattern: IUIAutomationValuePattern = focused
            .GetCurrentPattern(UIA_ValuePatternId)
            .map_err(|_| "focused element does not support ValuePattern".to_string())?
            .cast()
            .map_err(|_| "focused element's pattern is not a ValuePattern".to_string())?;

        let current = pattern
            .CurrentValue()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let new_value = format!("{current}{text}");

        pattern
            .SetValue(&windows::core::BSTR::from(new_value))
            .map_err(|e| format!("SetValue failed: {e}"))?;

        Ok(())
    }
}
