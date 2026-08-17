//! Actively triggers the macOS Accessibility and Input Monitoring
//! permission prompts, instead of relying on `rdev`'s global listener
//! silently doing nothing when access hasn't been granted — that's its
//! own documented behavior: "If the process is not granted access to the
//! Accessibility API, macOS will silently ignore rdev's `listen`
//! callback and will not trigger it with events. No error will be
//! generated." That silence is exactly what looks like "pressing the
//! hotkey does nothing" from the outside.
//!
//! Microphone permission doesn't need an explicit call here — CoreAudio
//! (via `cpal`) triggers that prompt itself the first time an input
//! stream is opened, *provided* Info.plist declares
//! `NSMicrophoneUsageDescription` (see `src-tauri/Info.plist` — without
//! it, macOS silently denies mic access instead of prompting, same
//! failure mode as Accessibility/Input Monitoring below).
//!
//! Caveat: granting Accessibility in System Settings often doesn't take
//! effect for an *already-running* process — the usual fix is granting
//! it, then relaunching the app once.

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::CFString;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use tracing::info;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDRequestAccess(request_type: u32) -> u8;
}

// Apple's own exported NSString constant identifying the audio media
// type, linked directly rather than hardcoded as a literal — its actual
// string value ("soun", a legacy four-character-code) isn't something
// worth risking a typo on when we can just reference the real symbol.
#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeAudio: *const AnyObject;
}

/// `AVAuthorizationStatus.authorized`, per AVFoundation's own enum.
const AV_AUTHORIZATION_STATUS_AUTHORIZED: isize = 3;

const K_IOHID_REQUEST_TYPE_LISTEN_EVENT: u32 = 1;

/// Checks Accessibility access, prompting the user if it hasn't been
/// decided yet. macOS renders its own system dialog and manages the
/// System Settings entry — there's nothing for us to build on top.
pub fn ensure_accessibility() {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();
    let options: CFDictionary<CFString, CFBoolean> = CFDictionary::from_CFType_pairs(&[(key, value)]);

    // SAFETY: `options` stays alive (owned by this stack frame) for the
    // duration of the call, and AXIsProcessTrustedWithOptions accepts a
    // nullable CFDictionaryRef per Apple's documented signature — we pass
    // a valid, non-null one.
    let trusted = unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
    info!(
        trusted = trusted != 0,
        "accessibility permission checked (prompts automatically if not yet granted)"
    );
}

/// Checks Input Monitoring access, prompting if not yet decided.
pub fn ensure_input_monitoring() {
    // SAFETY: takes a plain integer constant, no pointers involved; safe
    // to call from any thread per Apple's IOHIDLib documentation.
    let granted = unsafe { IOHIDRequestAccess(K_IOHID_REQUEST_TYPE_LISTEN_EVENT) };
    info!(
        granted = granted != 0,
        "input monitoring permission checked (prompts automatically if not yet decided)"
    );
}

/// Checks Accessibility access WITHOUT prompting — same underlying API
/// as `ensure_accessibility`, but with the prompt option left out
/// entirely (an empty options dict, same as passing NULL), so the
/// dashboard can poll this on a timer without ever side-effecting a
/// system dialog just from being open.
pub fn accessibility_authorized() -> bool {
    let options: CFDictionary<CFString, CFBoolean> = CFDictionary::from_CFType_pairs(&[]);
    // SAFETY: same call as `ensure_accessibility`, just with an empty
    // (still valid, non-null) options dictionary.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

/// Checks microphone access WITHOUT prompting. Unlike Accessibility,
/// there's no C API for this — `AVCaptureDevice`'s
/// `authorizationStatusForMediaType:` is the real (Objective-C) query
/// API, and it's a pure read: opening a capture stream (what
/// `audio::warm_up_microphone_permission` does) is the only thing that
/// prompts.
pub fn microphone_authorized() -> bool {
    // SAFETY: `class!` resolves a real, always-present system class
    // (AVFoundation is linked above); `AVMediaTypeAudio` is a valid,
    // non-null NSString constant from that same framework;
    // `authorizationStatusForMediaType:` is a pure query with no side
    // effects, safe to call from any thread.
    unsafe {
        let cls = class!(AVCaptureDevice);
        let status: isize = msg_send![cls, authorizationStatusForMediaType: AVMediaTypeAudio];
        status == AV_AUTHORIZATION_STATUS_AUTHORIZED
    }
}
