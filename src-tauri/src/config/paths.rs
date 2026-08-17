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

/// `<app_support_dir>/models` — gitignored. Populated either by hand (a
/// dev machine) or by `ensure_models_installed` copying them out of the
/// app bundle on first run. The per-language manifest/resumable-download
/// system from PROMPT.md §7 (for languages beyond the three bundled at
/// launch) still isn't implemented.
pub fn model_dir() -> PathBuf {
    app_support_dir().join("models")
}

/// First run only: if `model_dir()` doesn't already have the expected
/// models — a fresh install, since a dev machine typically already has
/// them placed by hand — copies them out of the app bundle's Resources
/// (`bundle.resources` in tauri.conf.json) into place. This is what lets
/// a downloaded `.app` work standalone with no separate model-download
/// step, at the cost of the bundle carrying ~250MB of ONNX weights.
/// Does real (blocking) file I/O — call from a blocking context, not the
/// async runtime or the main thread.
pub fn ensure_models_installed(app: &tauri::AppHandle) {
    use tauri::Manager;

    let dest = model_dir();
    if dest.join("sherpa-onnx-whisper-base").join("base-encoder.int8.onnx").exists() {
        return;
    }

    let resource_dir = match app.path().resource_dir() {
        Ok(dir) => dir,
        Err(err) => {
            tracing::warn!(
                ?err,
                "couldn't resolve the bundled resource dir — models won't auto-install \
                 (expected under `tauri dev`, where nothing gets bundled; a real problem \
                 in a built .app)"
            );
            return;
        }
    };
    let bundled = resource_dir.join("models");
    if !bundled.exists() {
        tracing::warn!(
            path = %bundled.display(),
            "no bundled models found at the expected resource path — expected under \
             `tauri dev`; a real problem in a built .app"
        );
        return;
    }

    tracing::info!("first run: copying bundled ASR models into place");
    match copy_dir_recursive(&bundled, &dest) {
        Ok(()) => tracing::info!(dest = %dest.display(), "ASR models installed"),
        Err(err) => tracing::error!(?err, "failed to copy bundled models into place"),
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
