//! The `rdev` global listener. Runs on its own dedicated OS thread — not a
//! Tokio task — because `rdev::listen` blocks and takes a `'static`
//! callback (PROMPT.md §6). Recognised hotkey transitions are forwarded
//! into the async world over an unbounded channel.

use std::time::Duration;

use rdev::{listen, Event, EventType};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::keymap::HotKey;

#[derive(Debug, Clone, Copy)]
pub enum RawKeyEvent {
    Down(HotKey),
    Up(HotKey),
}

const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(1);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(10);
const PERMISSION_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Blocks until Accessibility *and* Input Monitoring are both confirmed
/// granted. macOS only — Windows has no equivalent per-app gate, so this
/// returns immediately there.
///
/// `rdev::listen` does NOT error when either permission isn't granted yet
/// — per its own documented behavior (see `permissions::macos`'s doc
/// comment), it silently creates a dead event tap that blocks forever
/// *without ever delivering an event or returning*, no error raised. That
/// meant the retry-on-`Err` loop below couldn't help with the single most
/// common case: the normal onboarding flow is launch → dashboard opens →
/// user clicks Grant Access *while the app is already running* — so the
/// very first `listen()` call usually starts (and hangs, dead) before the
/// grant ever happens, and the retry loop never gets a failure to react
/// to. Waiting here, immediately before every `listen()` attempt, means a
/// tap only ever gets created once both permissions are confirmed real —
/// closing that gap without needing a relaunch.
///
/// Both, not just Accessibility: `rdev`'s tap is created at
/// `kCGHIDEventTap` (see `vendor/rdev-0.5.3/src/macos/listen.rs`), which
/// is gated by Input Monitoring — a separate, independently-grantable
/// permission from Accessibility. The dashboard's single "Accessibility"
/// button requests both at once, but nothing guarantees they're decided
/// by the user at the same moment, so both need to be waited on here.
fn wait_for_accessibility() {
    #[cfg(target_os = "macos")]
    {
        use crate::permissions::macos::{accessibility_authorized, input_monitoring_authorized};
        while !accessibility_authorized() || !input_monitoring_authorized() {
            std::thread::sleep(PERMISSION_POLL_INTERVAL);
        }
    }
}

pub fn spawn_listener() -> std::io::Result<mpsc::UnboundedReceiver<RawKeyEvent>> {
    let (tx, rx) = mpsc::unbounded_channel();

    std::thread::Builder::new()
        .name("speechx-hotkey-listener".into())
        .spawn(move || {
            // See `wait_for_accessibility`'s doc comment for why this
            // retry loop alone isn't sufficient, and why the wait is
            // needed before every attempt, not just logically "the
            // first" — a transient `Err` retry re-enters the same gap.
            let mut delay = INITIAL_RETRY_DELAY;
            loop {
                wait_for_accessibility();

                let tx = tx.clone();
                let callback = move |event: Event| {
                    let mapped = match event.event_type {
                        EventType::KeyPress(key) => HotKey::from_rdev(key).map(RawKeyEvent::Down),
                        EventType::KeyRelease(key) => HotKey::from_rdev(key).map(RawKeyEvent::Up),
                        _ => None,
                    };
                    if let Some(mapped) = mapped {
                        debug!(?mapped, "raw hotkey event");
                        // Receiver only drops when the app is shutting down; a
                        // failed send here just means there's nothing left to do.
                        let _ = tx.send(mapped);
                    }
                };
                match listen(callback) {
                    // `listen` blocks forever pumping its run loop; it
                    // only returns `Ok` if that loop exits cleanly, which
                    // doesn't happen in practice. Nothing left to retry.
                    Ok(()) => break,
                    Err(err) => {
                        warn!(
                            ?err,
                            retry_in = ?delay,
                            "rdev listener failed to start — check Accessibility & Input Monitoring permissions; retrying"
                        );
                        std::thread::sleep(delay);
                        delay = (delay * 2).min(MAX_RETRY_DELAY);
                    }
                }
            }
        })?;

    Ok(rx)
}
