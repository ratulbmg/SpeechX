use crate::rdev::{Event, EventType, ListenError};
use crate::windows::common::{convert, set_key_hook, set_mouse_hook, HookError, HOOK, KEYBOARD};
use std::os::raw::c_int;
use std::ptr::null_mut;
use std::time::SystemTime;
use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
use winapi::um::winuser::{CallNextHookEx, DispatchMessageA, GetMessageA, TranslateMessage, HC_ACTION, MSG};

static mut GLOBAL_CALLBACK: Option<Box<dyn FnMut(Event)>> = None;

impl From<HookError> for ListenError {
    fn from(error: HookError) -> Self {
        match error {
            HookError::Mouse(code) => ListenError::MouseHookError(code),
            HookError::Key(code) => ListenError::KeyHookError(code),
        }
    }
}

// Original rdev code, predating this lint (stabilized after rdev 0.5.3
// was published) — same pattern already silenced on the macOS side (see
// macos/listen.rs), for the same reason: not touching the access pattern
// itself, only the lint, to keep this vendored fork narrowly scoped to
// the one real fix it exists for (see macos/common.rs's SpeechX patch
// comment — this Windows file isn't part of that fix at all, but shares
// the same now-outdated `static mut` callback pattern).
#[allow(static_mut_refs)]
unsafe extern "system" fn raw_callback(code: c_int, param: WPARAM, lpdata: LPARAM) -> LRESULT {
    if code == HC_ACTION {
        let opt = convert(param, lpdata);
        if let Some(event_type) = opt {
            let name = match &event_type {
                EventType::KeyPress(_key) => match (*KEYBOARD).lock() {
                    Ok(mut keyboard) => keyboard.get_name(lpdata),
                    Err(_) => None,
                },
                _ => None,
            };
            let event = Event {
                event_type,
                time: SystemTime::now(),
                name,
            };
            if let Some(callback) = &mut GLOBAL_CALLBACK {
                callback(event);
            }
        }
    }
    CallNextHookEx(HOOK, code, param, lpdata)
}

pub fn listen<T>(callback: T) -> Result<(), ListenError>
where
    T: FnMut(Event) + 'static,
{
    unsafe {
        GLOBAL_CALLBACK = Some(Box::new(callback));
        set_key_hook(raw_callback)?;
        set_mouse_hook(raw_callback)?;

        // A proper Windows message pump is required for WH_KEYBOARD_LL hooks:
        // Windows delivers hook callbacks by posting to the installing thread's
        // message queue, so this thread must keep calling GetMessageA to service
        // them. The original code passed null for lpMsg (undefined behaviour —
        // GetMessageA returns -1 immediately on Windows), which caused listen()
        // to return Ok(()) right away, breaking out of the retry loop and killing
        // the listener thread before any hook events could ever be received.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageA(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
    }
    Ok(())
}
