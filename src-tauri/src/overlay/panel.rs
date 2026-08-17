//! The pill overlay panel (PROMPT.md §10 / M5).
//!
//! Built once, hidden, at app startup, then shown/hidden per dictation
//! session. A plain `WebviewWindowBuilder` window still activates the app
//! and steals focus on macOS — losing the target app's cursor position,
//! which is exactly what text injection needs (hazard #1, the highest-risk
//! item in the build). `tauri-nspanel` converts it into a non-activating
//! `NSPanel` instead: **verified** by holding the dictation hotkey with
//! the cursor in TextEdit and confirming it keeps blinking.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Position, Size, WebviewUrl};
use tauri_nspanel::{tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask};

use super::position::pill_position;

/// How long the "✓ <language>" match flash stays up before the pill
/// hides itself (PROMPT.md §10 pill states table).
const MATCH_FLASH_MS: u64 = 1200;

tauri_panel! {
    panel!(PillPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

pub const PILL_LABEL: &str = "pill";
const PILL_WIDTH: f64 = 180.0;
// Kept short deliberately — 84px (the original spec value) read as a
// slab, not a minimal overlay, once actually on screen. Half the width
// makes it a true pill/stadium shape (see the matching border-radius in
// pill.css).
const PILL_HEIGHT: f64 = 48.0;

/// Creates the pill panel, positioned but not yet shown. Idempotent-ish
/// in intent — call once from `setup()`; recreating an NSPanel per
/// session would be needless churn and risks re-triggering the
/// focus-theft hazard on every hold.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    PanelBuilder::<_, PillPanel>::new(app, PILL_LABEL)
        .url(WebviewUrl::App("src/pill.html".into()))
        .size(Size::Logical(LogicalSize::new(PILL_WIDTH, PILL_HEIGHT)))
        .has_shadow(false)
        .transparent(true)
        .corner_radius(PILL_HEIGHT / 2.0)
        .level(PanelLevel::ScreenSaver)
        // Built silently: no_activate keeps window creation from
        // momentarily bouncing the app to the foreground.
        .no_activate(true)
        // Without `.decorations(false)`, the underlying window is created
        // with the normal title bar (Tauri's default) and `style_mask`
        // below then strips the Titled bit *after* creation — AppKit can
        // throw on that post-hoc transition. Creating it borderless from
        // the start avoids the transition entirely.
        //
        // Without `.transparent(true)` *here* — Tauri's own webview-level
        // transparency flag, distinct from `PanelBuilder::transparent()`
        // above (which only clears the NSWindow's own background/opacity)
        // — the WKWebView's own backing layer stays opaque white
        // regardless of the window's transparency, so the pill shows as a
        // white box instead of vanishing when hidden. Available without
        // an explicit feature flag on our own `tauri` dependency because
        // `tauri-nspanel` itself depends on `tauri` with
        // `macos-private-api` enabled, and Cargo unifies that feature
        // across the build.
        .with_window(|w| w.decorations(false).transparent(true))
        .style_mask(StyleMask::empty().nonactivating_panel().borderless())
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .stationary()
                .full_screen_auxiliary(),
        )
        .build()?;

    reposition(app);
    Ok(())
}

/// Top-center, recomputed against whichever display is primary right
/// now — the user may have moved to (or plugged in) an external monitor
/// since the pill was last shown.
fn reposition(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PILL_LABEL) else {
        return;
    };
    let (x, y) = match window.primary_monitor() {
        Ok(Some(monitor)) => pill_position(
            (monitor.size().width, monitor.size().height),
            monitor.scale_factor(),
        ),
        _ => pill_position((1440, 900), 1.0),
    };
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

/// AppKit asserts that window-ordering calls happen on the main thread —
/// calling `panel.show()`/`.hide()` from the hotkey loop's tokio worker
/// thread crashes with `SIGTRAP` inside `-[NSWindow _doOrderWindow:]`.
/// `run_on_main_thread` posts the closure onto the run loop instead of
/// running it inline, so this is fire-and-forget from the caller's side.
fn show(app: &AppHandle) {
    let owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        reposition(&owned);
        if let Ok(panel) = owned.get_webview_panel(PILL_LABEL) {
            panel.show();
        }
    });
}

fn hide(app: &AppHandle) {
    let owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(panel) = owned.get_webview_panel(PILL_LABEL) {
            panel.hide();
        }
    });
}

/// Hides after `delay` on the Tauri-managed async runtime rather than
/// blocking whatever thread called us — session code calls these emit_*
/// functions from both the async hotkey loop and `spawn_blocking` ASR
/// pipelines.
fn hide_after(app: AppHandle, delay: Duration) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        hide(&app);
    });
}

#[derive(Serialize, Clone)]
struct ShowPayload<'a> {
    mode: &'a str,
    language: &'a str,
}

/// Chord resolved into a live recording session (PROMPT.md §10: label is
/// the active language's native name for Dictate, a prompt for Command).
pub fn emit_show(app: &AppHandle, mode: crate::hotkey::chord::Mode, language: &str) {
    let mode_str = match mode {
        crate::hotkey::chord::Mode::Dictate => "dictate",
        crate::hotkey::chord::Mode::Command => "command",
    };
    show(app);
    let _ = app.emit_to(PILL_LABEL, "pill-show", ShowPayload { mode: mode_str, language });
}

/// Recording stopped and ASR is running — frozen, 40% opacity, "…".
pub fn emit_transcribing(app: &AppHandle) {
    let _ = app.emit_to(PILL_LABEL, "pill-transcribing", ());
}

/// A dictate session finished (text injected or not) — nothing more to
/// show.
pub fn emit_hide(app: &AppHandle) {
    let _ = app.emit_to(PILL_LABEL, "pill-hide", ());
    hide(app);
}

/// Esc cancelled the session — instant fade, nothing typed.
pub fn emit_cancelled(app: &AppHandle) {
    let _ = app.emit_to(PILL_LABEL, "pill-cancelled", ());
    hide(app);
}

/// Command mode heard a language name clearly enough to switch — flash
/// "✓ <language>" in teal, then hide on its own.
pub fn emit_matched(app: &AppHandle, language: &str) {
    let _ = app.emit_to(PILL_LABEL, "pill-matched", language);
    hide_after(app.clone(), Duration::from_millis(MATCH_FLASH_MS));
}

/// Smoothed 0..1 microphone level, forwarded from the cpal capture
/// callback while a session is recording (PROMPT.md §10 audio-level
/// pipeline). Throttled to ~30 Hz by the caller, not here.
pub fn emit_audio_level(app: &AppHandle, level: f32) {
    let _ = app.emit_to(PILL_LABEL, "audio-level", level);
}
