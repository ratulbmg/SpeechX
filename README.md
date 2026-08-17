# SpeechX

Offline, background dictation for macOS and Windows. Hold a key, speak, let go — your
words are typed wherever your cursor is, in whatever app you're using.

Everything runs **fully offline** (Whisper + a streaming Zipformer model, via
`sherpa-onnx`). No internet connection is used for speech recognition, and nothing you
say ever leaves your machine. English, Hindi, and Bengali are supported, with the
ability to switch languages by voice mid-session.

## Supported Languages

| Language | Say | Engine |
|---|---|---|
| English | "English" | Whisper |
| Hindi | "Hindi" / हिंदी | Whisper |
| Bengali | "Bengali" / বাংলা | Streaming Zipformer (dedicated Bengali model) |

English and Hindi are both served by the same Whisper model (reloaded with a different
language setting when you switch). Bengali gets its own dedicated model instead of being
routed through Whisper, for meaningfully better accuracy. Say the language name (in
English or the language's own script) while switching — see [Usage](#usage) below.

## Download & Install

Grab the latest build from [Releases](https://github.com/ratulbmg/SpeechX/releases/latest).

### macOS

1. Download the `.dmg`, open it, and drag `SpeechX.app` into `Applications`.
2. **Important:** the app isn't notarized by Apple yet (that requires a paid Apple
   Developer account, which this project doesn't have as of now) — so macOS's Gatekeeper
   blocks a plain double-click on first launch with an "Apple could not verify..."
   message. This is expected for any unsigned/un-notarized app, not a bug. Two ways
   around it:
   - **Terminal (recommended, one-time per install):**
     ```bash
     xattr -cr /Applications/SpeechX.app
     ```
     This clears the quarantine flag macOS attaches to anything downloaded via a
     browser/Finder. After running it, double-click the app normally — no dialog.
   - **Without Terminal:** try right-clicking `SpeechX.app` → **Open** → **Open** on the
     confirmation dialog. On some macOS versions this skips the block; on newer versions
     (Sequoia and later) it often still routes through the same block, in which case go to
     **System Settings → Privacy & Security**, scroll down, and click **Open Anyway** next
     to the message about SpeechX (only appears after you've tried opening it once).
3. Launch it, open the dashboard from the menu bar icon, and grant Accessibility and
   Microphone access under the **Permissions** tab (in that order — Accessibility first).

### Windows

1. Download the `.exe` (NSIS) or `.msi` installer and run it.
2. Some antivirus software flags apps that use global hotkeys — this is a known false
   positive for this category of app (a global low-level keyboard hook is exactly what
   both dictation apps and keyloggers use, so heuristic scanners can't tell them apart
   from the hook alone).

## Usage

Three keys, all on the **right-hand** modifiers (deliberately — left-side modifiers are
never used, so they never collide with normal shortcuts like Cmd/Ctrl+C):

| Action | macOS | Windows | What happens |
|---|---|---|---|
| Dictate | Hold **Right ⌘** | Hold **Right Alt** | Speak, release — the transcript is typed at your cursor |
| Switch language | Hold **Right ⌘ + Right ⌥**, say a language name | Hold **Right Alt + Right Ctrl**, say a language name | Say "English", "Hindi", or "Bengali" to switch |
| Cancel | **Esc** (while holding the trigger) | **Esc** (while holding the trigger) | Discards the current recording, types nothing |

SpeechX runs as a background utility with no Dock icon — look for its icon in the menu
bar (top-right on macOS) to open the dashboard, toggle listening on/off, or quit.

## Contributing

Want to build SpeechX from source or make changes? See [`CONTRIBUTING.md`](./CONTRIBUTING.md).
