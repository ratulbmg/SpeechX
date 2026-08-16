//! Windows text injection via `SendInput` with `KEYEVENTF_UNICODE`.
//! This is the Windows equivalent of macOS's `CGEventKeyboardSetUnicodeString`
//! (WINDOWS_SUPPORT.md §5) — it posts UTF-16 code units directly, so
//! Devanagari/Bengali work exactly like English, no layout mapping needed.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY,
};

pub fn inject_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

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
