//! The menu bar (tray) icon — the primary way back into the dashboard
//! once its window is closed. SpeechX runs with no Dock icon
//! (`ActivationPolicy::Accessory`, set in `lib.rs`) so the window doesn't
//! steal focus from whatever app the user is dictating into, but that
//! means there's otherwise nothing to click to bring the dashboard back.
//!
//! Offers "Open Dashboard", the same Listening toggle as the dashboard
//! (kept in sync both ways through `commands::ListeningState`), and
//! "Quit SpeechX".

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::commands::ListeningState;

const ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let listening_state = app.state::<ListeningState>();
    let initially_checked = listening_state.enabled.load(std::sync::atomic::Ordering::Relaxed);

    let open_item = MenuItem::with_id(app, "open_dashboard", "Open Dashboard", true, None::<&str>)?;
    let listening_item = CheckMenuItem::with_id(
        app,
        "toggle_listening",
        "Listening",
        true,
        initially_checked,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit SpeechX", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &PredefinedMenuItem::separator(app)?,
            &listening_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    // Stashed in managed state so `commands::set_listening_enabled` (the
    // dashboard's own toggle) can tick/untick this same checkbox when the
    // change comes from the *other* UI surface.
    listening_state.set_tray_checkbox(listening_item);

    let icon = tauri::image::Image::from_bytes(ICON_BYTES)?;

    TrayIconBuilder::new()
        .icon(icon)
        // A template image lets macOS recolor it automatically for the
        // light/dark menu bar instead of showing a fixed black glyph that
        // goes invisible in dark mode.
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_dashboard" => show_dashboard(app),
            "toggle_listening" => {
                let state = app.state::<ListeningState>();
                let next = !state.enabled.load(std::sync::atomic::Ordering::Relaxed);
                state.set_listening(next);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Also the single-instance handler's fallback (`lib.rs`) — both a tray
/// click and a second launch attempt land here, since both mean the same
/// thing: the user wants the dashboard back.
pub fn show_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
