//! Windows text injection via `SendInput` with `KEYEVENTF_UNICODE` (ASCII)
//! or clipboard + Ctrl+V (non-ASCII / complex scripts).
//!
//! `KEYEVENTF_UNICODE` events for complex-script characters (Bengali, Hindi,
//! Arabic, etc.) can trigger Windows TSF/IME **composition mode** in some
//! applications. During composition the text is drawn with a dark composition
//! overlay, making it appear black until the user presses another key (e.g.
//! Space) which commits the composition. Routing non-ASCII text through the
//! clipboard bypasses the IME pipeline entirely and renders correctly from the
//! first character.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

pub fn inject_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

    // Non-ASCII characters (Bengali, Hindi, Arabic, CJK, etc.) sent via
    // KEYEVENTF_UNICODE can activate TSF/IME composition mode, causing them
    // to render with a dark composition overlay until a key commits the
    // composition. Clipboard + Ctrl+V bypasses IME and renders correctly.
    if text.chars().any(|c| !c.is_ascii()) {
        return inject_via_clipboard(text);
    }

    inject_via_sendinput(text)
}

fn inject_via_sendinput(text: &str) -> Result<(), String> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 2);
    let mut utf16_buf = [0u16; 2];

    for ch in text.chars() {
        // Characters outside the Basic Multilingual Plane must be sent as
        // UTF-16 surrogate pairs; `encode_utf16` handles that for us.
        for &unit in ch.encode_utf16(&mut utf16_buf).iter() {
            inputs.push(make_unicode_input(unit, false));
            inputs.push(make_unicode_input(unit, true));
        }
    }

    let written = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };

    if (written as usize) < inputs.len() {
        Err(format!(
            "SendInput only accepted {written}/{} events — may be blocked by a secure input field",
            inputs.len()
        ))
    } else {
        Ok(())
    }
}

fn inject_via_clipboard(text: &str) -> Result<(), String> {
    unsafe {
        let hwnd = GetForegroundWindow();

        // Open the clipboard on the foreground window's thread context.
        OpenClipboard(hwnd).map_err(|e| format!("OpenClipboard failed: {e}"))?;

        let clipboard_result = (|| {
            EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;

            // CF_UNICODETEXT expects a null-terminated UTF-16 string in a
            // global memory block.
            let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0u16)).collect();
            let byte_len = utf16.len() * std::mem::size_of::<u16>();

            let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len)
                .map_err(|e| format!("GlobalAlloc failed: {e}"))?;

            let ptr = GlobalLock(hmem);
            if ptr.is_null() {
                return Err("GlobalLock returned null".to_string());
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const u8, ptr as *mut u8, byte_len);
            let _ = GlobalUnlock(hmem);

            // CF_UNICODETEXT = 13
            SetClipboardData(13, HANDLE(hmem.0))
                .map_err(|e| format!("SetClipboardData failed: {e}"))?;

            Ok(())
        })();

        let _ = CloseClipboard();
        clipboard_result?;
    }

    // Send Ctrl+V to paste.
    let inputs = [
        make_vk_input(VK_CONTROL, false),
        make_vk_input(VK_V, false),
        make_vk_input(VK_V, true),
        make_vk_input(VK_CONTROL, true),
    ];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };

    Ok(())
}

fn make_unicode_input(unicode_unit: u16, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_UNICODE
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unicode_unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn make_vk_input(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags = if key_up { KEYEVENTF_KEYUP } else { Default::default() };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
