//! The full `Settings` struct (load/save/defaults/migration, PROMPT.md
//! §12) is M8 scope and doesn't exist yet. This currently holds only
//! launch-at-login, needed for WINDOWS_SUPPORT.md §8.

// No caller yet — there's no settings UI (M8) to flip this from. Kept
// implemented (not deleted) since it's real, working WINDOWS_SUPPORT.md
// §8 code, just not wired to anything that calls it today.
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_SET_VALUE)
        .map_err(|e| format!("failed to open registry Run key: {e}"))?;

    if enabled {
        let exe_path = std::env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
        run_key
            .set_value("SpeechX", &exe_path.to_string_lossy().as_ref())
            .map_err(|e| format!("failed to set registry value: {e}"))?;
    } else {
        // Ignore "not present" — disabling when already disabled isn't an error.
        let _ = run_key.delete_value("SpeechX");
    }

    Ok(())
}

/// macOS launch-at-login (Login Items via `SMAppService`) is M8 scope —
/// not implemented yet, on either platform.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub fn set_launch_at_login(_enabled: bool) -> Result<(), String> {
    Err("launch-at-login is not implemented yet on macOS (M8)".into())
}
