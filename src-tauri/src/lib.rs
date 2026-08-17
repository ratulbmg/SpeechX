#![deny(unsafe_op_in_unsafe_fn)]

mod asr;
mod audio;
mod config;
mod hotkey;
mod inject;
mod lang;
mod overlay;
mod permissions;

use tracing_subscriber::EnvFilter;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
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
        .plugin(tauri_plugin_single_instance::init(|_app, args, cwd| {
            tracing::warn!(?args, ?cwd, "a second SpeechX instance tried to start — ignoring it, this one keeps running");
        }))
        .plugin(tauri_plugin_opener::init());
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .setup(|app| {
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

            // Trigger the Accessibility / Input Monitoring system prompts
            // up front rather than letting the hotkey listener silently
            // receive nothing if they're not yet granted (see
            // `permissions::macos`'s doc comment).
            #[cfg(target_os = "macos")]
            {
                permissions::macos::ensure_accessibility();
                permissions::macos::ensure_input_monitoring();
            }

            // Same idea for the microphone prompt: without this, it only
            // fires the first time the user actually holds the hotkey —
            // a confusing moment for a permission dialog to interrupt.
            // Opening (then immediately closing) a stream at launch
            // brings all three permission prompts together right after
            // install. Blocking (waits for CoreAudio, and however long
            // the user takes to answer the dialog), so it runs on its
            // own thread rather than holding up the rest of setup().
            tauri::async_runtime::spawn_blocking(audio::warm_up_microphone_permission);

            // The pill overlay (PROMPT.md §10 / M5): built once, hidden,
            // then shown/hidden per dictation session by the hotkey loop.
            #[cfg(target_os = "macos")]
            overlay::panel::create(app.handle())?;

            tauri::async_runtime::spawn(hotkey::run(app.handle().clone()));
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
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
