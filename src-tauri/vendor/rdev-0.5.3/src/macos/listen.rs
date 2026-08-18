#![allow(improper_ctypes_definitions)]
use crate::macos::common::*;
use crate::rdev::{Event, ListenError};
use cocoa::base::nil;
use cocoa::foundation::NSAutoreleasePool;
use core_graphics::event::{CGEventTapLocation, CGEventType};
use std::os::raw::c_void;

static mut GLOBAL_CALLBACK: Option<Box<dyn FnMut(Event)>> = None;

// SpeechX patch: macOS can and does automatically disable a `ListenOnly`
// event tap — most commonly after `kCGEventTapDisabledByTimeout` (the
// callback didn't return fast enough on some pass, even transiently) or
// `kCGEventTapDisabledByUserInput`, both of which are real `CGEventType`
// values delivered to the same callback, not just theoretical. Original
// rdev never checks for either: `convert()` in common.rs has no match arm
// for them, so they silently fall through to `None` and get dropped —
// nothing ever calls `CGEventTapEnable` again. The tap then stays dead
// for the rest of the process's life; every subsequent keypress is
// received by macOS and handed to `raw_callback`, but produces no `Event`
// at all. This is exactly what made SpeechX's hotkey "stop listening"
// after switching focus away from it — not a permissions problem, a
// silently-disabled tap the app never noticed. `GLOBAL_TAP` gives
// `raw_callback` a handle to re-enable itself when this happens.
static mut GLOBAL_TAP: CFMachPortRef = std::ptr::null();

#[link(name = "Cocoa", kind = "framework")]
extern "C" {}

// Original rdev code, predating this lint (stabilized after rdev 0.5.3
// was published) — same single-threaded-callback pattern as before,
// just now flagged as technically-riskier-in-general than a raw pointer
// access. Not changing the access pattern itself, only silencing the
// lint, to keep this patch narrowly scoped to the one real fix it exists
// for (see common.rs's SpeechX patch comment).
#[allow(static_mut_refs)]
unsafe extern "C" fn raw_callback(
    _proxy: CGEventTapProxy,
    _type: CGEventType,
    cg_event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    // SpeechX patch: see `GLOBAL_TAP`'s doc comment above — these two
    // variants mean the OS just disabled the tap, not a real input event;
    // `convert()` has no arm for them and would just drop them, leaving
    // the tap dead. Re-enable immediately and skip normal handling.
    if matches!(
        _type,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        if !GLOBAL_TAP.is_null() {
            CGEventTapEnable(GLOBAL_TAP, true);
        }
        return cg_event;
    }

    // println!("Event ref {:?}", cg_event_ptr);
    // let cg_event: CGEvent = transmute_copy::<*mut c_void, CGEvent>(&cg_event_ptr);
    let opt = KEYBOARD_STATE.lock();
    if let Ok(mut keyboard) = opt {
        if let Some(event) = convert(_type, &cg_event, &mut keyboard) {
            if let Some(callback) = &mut GLOBAL_CALLBACK {
                callback(event);
            }
        }
    }
    // println!("Event ref END {:?}", cg_event_ptr);
    // cg_event_ptr
    cg_event
}

pub fn listen<T>(callback: T) -> Result<(), ListenError>
where
    T: FnMut(Event) + 'static,
{
    unsafe {
        GLOBAL_CALLBACK = Some(Box::new(callback));
        let _pool = NSAutoreleasePool::new(nil);
        let tap = CGEventTapCreate(
            CGEventTapLocation::HID, // HID, Session, AnnotatedSession,
            kCGHeadInsertEventTap,
            CGEventTapOption::ListenOnly,
            kCGEventMaskForAllEvents,
            raw_callback,
            nil,
        );
        if tap.is_null() {
            return Err(ListenError::EventTapError);
        }
        // SpeechX patch: see `GLOBAL_TAP`'s doc comment above.
        GLOBAL_TAP = tap;
        let _loop = CFMachPortCreateRunLoopSource(nil, tap, 0);
        if _loop.is_null() {
            return Err(ListenError::LoopSourceError);
        }

        let current_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(current_loop, _loop, kCFRunLoopCommonModes);

        CGEventTapEnable(tap, true);
        CFRunLoopRun();
    }
    Ok(())
}
