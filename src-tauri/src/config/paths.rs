use std::path::PathBuf;

/// `~/Library/Application Support/SpeechX` on macOS, `%APPDATA%\SpeechX`
/// (i.e. `C:\Users\<name>\AppData\Roaming\SpeechX`) on Windows
/// (PROMPT.md §3, §12; WINDOWS_SUPPORT.md §15).
///
/// Tauri's `app.path().app_data_dir()` gives the same answer and is the
/// more idiomatic source once code has an `AppHandle` — this free
/// function exists because `asr::manager`'s model loader runs from a
/// `OnceLock` outside any Tauri command context.
pub fn app_support_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("SpeechX")
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Application Support/SpeechX")
    }
}

/// `<app_support_dir>/models` — gitignored, populated by the model
/// downloader (PROMPT.md §7 manifest/downloader, not yet implemented;
/// today's Whisper base model was fetched by hand).
pub fn model_dir() -> PathBuf {
    app_support_dir().join("models")
}
