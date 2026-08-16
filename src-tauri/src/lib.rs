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

    tauri::Builder::default()
        // Must be registered first (Tauri's requirement). Without this, a
        // second launch — e.g. two overlapping `tauri dev` runs during
        // development, or a user double-clicking the app icon twice —
        // starts a second global hotkey listener and a second dictation
        // pipeline. Both fire on the same keypress and both inject text,
        // which is exactly what produces duplicated output.
        .plugin(tauri_plugin_single_instance::init(|_app, args, cwd| {
            tracing::warn!(?args, ?cwd, "a second SpeechX instance tried to start — ignoring it, this one keeps running");
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            // Trigger the Accessibility / Input Monitoring system prompts
            // up front rather than letting the hotkey listener silently
            // receive nothing if they're not yet granted (see
            // `permissions::macos`'s doc comment). Microphone prompting
            // happens on its own via CoreAudio once Info.plist declares
            // NSMicrophoneUsageDescription.
            #[cfg(target_os = "macos")]
            {
                permissions::macos::ensure_accessibility();
                permissions::macos::ensure_input_monitoring();
            }

            tauri::async_runtime::spawn(hotkey::run());
            // Load + warm the command-recognition engine and both
            // dictation engines (English/Whisper, Bengali/Zipformer) up
            // front, so no first use — dictation or language switch —
            // pays for model load time. Hindi shares Whisper with
            // English and only gets (re)loaded on first actual use.
            tauri::async_runtime::spawn_blocking(|| {
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
