#![deny(unsafe_op_in_unsafe_fn)]

mod asr;
mod audio;
mod commands;
mod config;
mod hotkey;
mod inject;
mod lang;
mod overlay;
mod permissions;
mod tray;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Requests Accessibility, then Input Monitoring, then Microphone, in
/// that order — each one only requested once the previous is confirmed
/// granted, automatically at launch, rather than waiting for the user to
/// click through the dashboard's Permissions tab (which is now a
/// read-only status display, not a set of action buttons — see
/// `src/App.tsx`). A step whose permission is already granted (a second
/// launch, say) skips straight past it: `ensure_accessibility`/
/// `ensure_input_monitoring` are themselves no-ops when already trusted,
/// and the loop below exits on its very first check.
///
/// This ordering isn't arbitrary: it's also a mitigation for a real bug
/// found earlier — `IOHIDRequestAccess` (Input Monitoring's prompt) can
/// silently fail to appear at all if `AXIsProcessTrustedWithOptions`
/// (Accessibility's check) has already been polled aggressively earlier
/// in the process's life. Waiting for Accessibility to fully resolve
/// before ever touching Input Monitoring keeps that interaction as
/// clean as possible, though it isn't a guaranteed fix — the dashboard's
/// "Manual steps" link is still there for when a prompt doesn't show.
///
/// macOS-only: Windows has no equivalent consent flow to sequence (see
/// `permissions::macos`'s module doc comment) — its one relevant call,
/// the microphone warm-up, has no user-facing prompt to sequence around
/// at all, so it just runs directly wherever this is called from.
#[cfg(target_os = "macos")]
async fn onboard_permissions_sequentially() {
    use permissions::macos::{
        accessibility_authorized, ensure_accessibility, ensure_input_monitoring, input_monitoring_authorized,
    };

    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(800);

    ensure_accessibility();
    while !accessibility_authorized() {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    tracing::info!("accessibility granted — requesting input monitoring next");

    ensure_input_monitoring();
    while !input_monitoring_authorized() {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    tracing::info!("input monitoring granted — requesting microphone next");

    audio::warm_up_microphone_permission();
    tracing::info!("permission onboarding sequence complete");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("speechx=debug")),
        )
        .init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        // Must be registered first (Tauri's requirement). Without this, a
        // second launch — e.g. two overlapping `tauri dev` runs during
        // development, or a user double-clicking the app icon twice —
        // starts a second global hotkey listener and a second dictation
        // pipeline. Both fire on the same keypress and both inject text,
        // which is exactly what produces duplicated output.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            tracing::info!(?args, ?cwd, "second launch attempt — showing the existing dashboard instead");
            tray::show_dashboard(app);
        }))
        .plugin(tauri_plugin_opener::init());
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    // Dashboard-controlled master switch (src/App.tsx's "Listening"
    // toggle) — read by the hotkey loop on every Right Command press.
    let listening_enabled = Arc::new(AtomicBool::new(true));
    // Dashboard-controlled microphone picker (Controls tab) — `None`
    // means "System Default". Read fresh by `AudioController` on every
    // dictation session, so switching devices takes effect immediately.
    let selected_microphone: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    builder
        .manage(commands::ListeningState::new(listening_enabled.clone()))
        .manage(commands::SelectedMicrophone(selected_microphone.clone()))
        .setup(move |app| {
            // SpeechX is meant to run as a background/menu-bar utility
            // (PROMPT.md §1) — without this, Tauri defaults to
            // `NSApplicationActivationPolicyRegular`, a normal foreground
            // app. That gives it a Dock icon and, critically, makes
            // *launching* SpeechX steal activation from whatever app the
            // user was in, and keeps stealing it back — so the "focused
            // application" enigo injects into is SpeechX's own hidden
            // window, not the app the user actually placed their cursor
            // in. `Accessory` policy fixes both: no Dock icon, and the
            // app never becomes the active/frontmost one, so the user's
            // real target app keeps focus throughout.
            #[cfg(target_os = "macos")]
            app.handle().set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            // With no Dock icon, the menu bar icon (`tray.rs`) is the only
            // permanent way back into the dashboard, so its "Open
            // Dashboard" item (and a second launch attempt via the
            // single-instance handler above) both need the window to
            // still exist, not just be closed. The red traffic-light
            // button's default behavior is to *destroy* the window, which
            // would leave both of those with nothing to show — so the
            // close button is intercepted here to hide it instead. The
            // background process (hotkey listener, etc.) was already
            // unaffected by closing the window either way; this just
            // makes the window itself recoverable too.
            if let Some(window) = app.get_webview_window("main") {
                // Belt-and-suspenders with `Info.plist`'s
                // `NSQuitAlwaysKeepsWindows` (which only affects whether
                // the OS *saves* state to restore): this is the actual
                // NSWindow-level flag Apple's restoration system checks
                // per-window. Without it, a prior abrupt termination
                // (a force-quit, `kill`, or `app.exit()`'s own abrupt
                // process exit — all routine for a background utility
                // like this one) can leave macOS's Resume feature
                // thinking a restoration is owed, showing "SpeechX quit
                // unexpectedly while reopening windows" on a later
                // launch. SpeechX has no session state worth restoring
                // (the dashboard reopens on demand via the tray icon), so
                // it opts the window out of restoration entirely, at the
                // AppKit level directly, rather than relying on the
                // Info.plist preference alone.
                #[cfg(target_os = "macos")]
                {
                    use objc2::msg_send;
                    use objc2::runtime::AnyObject;

                    if let Ok(ns_window_ptr) = window.ns_window() {
                        // SAFETY: `ns_window()` hands back a valid,
                        // non-null NSWindow* for as long as the window
                        // exists (Tauri's own contract for this method);
                        // `setRestorable:` is a plain property setter, no
                        // side effects beyond flipping that flag.
                        unsafe {
                            let ns_window = ns_window_ptr as *mut AnyObject;
                            let _: () = msg_send![ns_window, setRestorable: false];
                        }
                    }
                }

                let window_to_hide = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }
            tray::create(app.handle())?;

            // Accessibility, then Input Monitoring, then Microphone — asked
            // for automatically, one at a time, right at launch (see
            // `onboard_permissions_sequentially` above). The dashboard's
            // Permissions tab is now a read-only status display, not a set
            // of buttons the user has to click through. Until Accessibility
            // is actually granted, the hotkey listener below just sits
            // retrying in the background (see `listener.rs`'s documented
            // backoff loop); it starts working within seconds of the
            // grant, whenever that happens.
            #[cfg(target_os = "macos")]
            tauri::async_runtime::spawn(onboard_permissions_sequentially());
            // Windows has no Accessibility/Input Monitoring equivalent to
            // sequence in front of this (see `permissions::macos`'s module
            // doc comment) — just trigger the one prompt it does have
            // (the system-wide microphone privacy toggle) directly at
            // launch, same as the last step of the macOS chain above.
            #[cfg(not(target_os = "macos"))]
            tauri::async_runtime::spawn_blocking(audio::warm_up_microphone_permission);

            let hotkey_app = app.handle().clone();
            let hotkey_listening_enabled = listening_enabled.clone();
            let hotkey_selected_microphone = selected_microphone.clone();
            tauri::async_runtime::spawn(hotkey::run(hotkey_app, hotkey_listening_enabled, hotkey_selected_microphone));

            // The pill overlay (PROMPT.md §10 / M5): built once, hidden,
            // then shown/hidden per dictation session by the hotkey loop.
            #[cfg(target_os = "macos")]
            overlay::panel::create(app.handle())?;
            #[cfg(target_os = "windows")]
            overlay::pill_windows::create(app.handle())?;

            // Load + warm the command-recognition engine and both
            // dictation engines (English/Whisper, Bengali/Zipformer) up
            // front, so no first use — dictation or language switch —
            // pays for model load time. Hindi shares Whisper with
            // English and only gets (re)loaded on first actual use.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                // First run on a fresh install: copies the bundled models
                // into place before anything tries to load them. A no-op
                // on a dev machine that already has them.
                config::paths::ensure_models_installed(&app_handle);

                if let Err(err) = asr::manager::command_engine() {
                    tracing::error!(?err, "failed to preload command-mode whisper model");
                }
                if let Err(err) = asr::manager::dictate_engine(asr::engine::LanguageCode::En) {
                    tracing::error!(?err, "failed to preload English dictate model");
                }
                if let Err(err) = asr::manager::dictate_engine(asr::engine::LanguageCode::Bn) {
                    tracing::error!(?err, "failed to preload Bengali dictate model");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::check_microphone_permission,
            commands::check_accessibility_permission,
            commands::check_input_monitoring_permission,
            commands::get_listening_enabled,
            commands::set_listening_enabled,
            commands::list_microphones,
            commands::get_selected_microphone,
            commands::set_selected_microphone,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
