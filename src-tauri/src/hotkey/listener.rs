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

pub fn spawn_listener() -> std::io::Result<mpsc::UnboundedReceiver<RawKeyEvent>> {
    let (tx, rx) = mpsc::unbounded_channel();

    std::thread::Builder::new()
        .name("speechx-hotkey-listener".into())
        .spawn(move || {
            // Creating the event tap requires Accessibility + Input
            // Monitoring to already be granted. On a fresh install, the
            // permission dialogs `permissions::macos` triggers at startup
            // take the user a few seconds to click through — by the time
            // they answer, a single `listen()` attempt has usually
            // already failed, since rdev doesn't retry internally and
            // just returns an error immediately. Without a retry here,
            // granting permission moments later does nothing: this
            // thread would already be dead for the rest of the session,
            // and only a full quit+relaunch would pick up the grant.
            // Retrying with capped backoff instead means it starts
            // working within seconds of permission being granted,
            // whenever that happens, with no relaunch needed.
            let mut delay = INITIAL_RETRY_DELAY;
            loop {
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
