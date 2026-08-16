use rdev::Key;

/// Semantic hotkeys SpeechX reacts to, decoupled from `rdev::Key` so a
/// future rebind (settings.toml `[hotkeys]`) doesn't leak into chord logic.
/// Names describe the *role* each key plays (dictate trigger / command
/// modifier), not a specific physical key — the mapping below is what
/// ties a role to an actual key, and it differs per platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotKey {
    /// Dictate trigger: Right Command on macOS, Right Alt on Windows
    /// (Windows keyboards have no Command key).
    RightCommand,
    /// Held alongside the trigger to enter Command mode: Right Option on
    /// macOS, Right Ctrl on Windows.
    RightOption,
    Escape,
}

impl HotKey {
    /// macOS CGKeyCodes, for reference against PROMPT.md §6:
    /// RightCommand = 54, LeftCommand = 55, RightOption = 61, LeftOption = 58, Escape = 53.
    /// `rdev`'s macOS backend maps those to `MetaRight`, `MetaLeft`, `AltGr`, `Alt`,
    /// and `Escape` respectively — verified against rdev's own keycode table.
    #[cfg(target_os = "macos")]
    pub fn from_rdev(key: Key) -> Option<Self> {
        match key {
            Key::MetaRight => Some(Self::RightCommand),
            Key::AltGr => Some(Self::RightOption),
            Key::Escape => Some(Self::Escape),
            // Deliberately no LeftCommand (Key::MetaLeft) — it collides with
            // every standard shortcut.
            _ => None,
        }
    }

    /// Windows VK codes, per `rdev`'s own table (verified in
    /// `rdev::windows::keycodes`): VK_LMENU=164 -> `Key::Alt` (left Alt),
    /// VK_RMENU=165 -> `Key::AltGr` (right Alt), VK_RCONTROL=163 ->
    /// `Key::ControlRight`. WINDOWS_SUPPORT.md §7 named `Key::Alt` for the
    /// dictate trigger, but that's *left* Alt in rdev's own mapping —
    /// `Key::AltGr` is the right one here.
    #[cfg(target_os = "windows")]
    pub fn from_rdev(key: Key) -> Option<Self> {
        match key {
            Key::AltGr => Some(Self::RightCommand),
            Key::ControlRight => Some(Self::RightOption),
            Key::Escape => Some(Self::Escape),
            // Deliberately no plain ControlLeft/Alt — they collide with
            // every standard shortcut.
            _ => None,
        }
    }
}
