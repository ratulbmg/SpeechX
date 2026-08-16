//! The `rdev` global listener. Runs on its own dedicated OS thread — not a
//! Tokio task — because `rdev::listen` blocks and takes a `'static`
//! callback (PROMPT.md §6). Recognised hotkey transitions are forwarded
//! into the async world over an unbounded channel.

use rdev::{listen, Event, EventType};
use tokio::sync::mpsc;
use tracing::{debug, error};

use super::keymap::HotKey;

#[derive(Debug, Clone, Copy)]
pub enum RawKeyEvent {
    Down(HotKey),
    Up(HotKey),
}

pub fn spawn_listener() -> std::io::Result<mpsc::UnboundedReceiver<RawKeyEvent>> {
    let (tx, rx) = mpsc::unbounded_channel();

    std::thread::Builder::new()
        .name("speechx-hotkey-listener".into())
        .spawn(move || {
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
            if let Err(err) = listen(callback) {
                error!(
                    ?err,
                    "rdev listener terminated — check Accessibility & Input Monitoring permissions"
                );
            }
        })?;

    Ok(rx)
}
