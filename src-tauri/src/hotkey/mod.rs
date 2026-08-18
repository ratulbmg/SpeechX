pub mod chord;
pub mod keymap;
pub mod listener;

use std::future::pending;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::AppHandle;
use tokio::time::Instant;
use tracing::info;

use chord::{ChordEvent, ChordMachine, ChordOutcome, Mode, Phase, ARM_WINDOW_MS};
use keymap::HotKey;
use listener::RawKeyEvent;

use crate::asr::engine::LanguageCode;
use crate::audio::AudioController;

fn now_ms() -> u64 {
    // Session-relative wall clock; the chord machine only ever compares
    // this against other values it was handed, never against a real epoch.
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = *START.get_or_init(std::time::Instant::now);
    start.elapsed().as_millis() as u64
}

/// Reads the active language, recovering rather than panicking if a
/// prior panic poisoned the lock — a stale-but-valid language beats
/// crashing the whole hotkey loop.
fn read_active_language(lock: &Mutex<LanguageCode>) -> LanguageCode {
    *lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Drives the chord state machine from real hotkey events. Dictate
/// sessions run through ASR + text injection; Command sessions run
/// through ASR + the language matcher and update `active_language`.
///
/// `listening_enabled` is the dashboard's "Listening" toggle
/// (`commands::ListeningState`) — checked on every Right Command press so
/// disabling it stops new sessions from arming. A session already
/// in-flight when it flips off is left to finish normally; only the next
/// press is blocked.
pub async fn run(app: AppHandle, listening_enabled: Arc<AtomicBool>) {
    let mut rx = match listener::spawn_listener() {
        Ok(rx) => rx,
        Err(err) => {
            tracing::error!(?err, "failed to start hotkey listener");
            return;
        }
    };

    info!("hotkey listener attached — hold Right Cmd to dictate, Right Cmd + Right Option to switch language");

    let mut machine = ChordMachine::new();
    let mut audio = AudioController::new(app.clone());
    let mut arm_deadline: Option<Instant> = None;
    let active_language = Arc::new(Mutex::new(LanguageCode::En));

    loop {
        let timer = async {
            match arm_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => pending::<()>().await,
            }
        };

        tokio::select! {
            biased;

            _ = timer => {
                arm_deadline = None;
                if let ChordOutcome::ModeResolved(mode) = machine.on_event(ChordEvent::ArmTimerFired { at_ms: now_ms() }) {
                    log_mode(mode);
                    show_pill(&app, mode, &active_language);
                }
            }

            maybe_event = rx.recv() => {
                let Some(raw) = maybe_event else {
                    tracing::warn!("hotkey listener channel closed — stopping");
                    break;
                };
                handle_raw_event(raw, &mut machine, &mut arm_deadline, &mut audio, &active_language, &app, &listening_enabled);
            }
        }
    }
}

/// Looks up the active language's native-script display name (e.g. `বাংলা`)
/// for the pill label — falls back to the code itself if `languages.toml`
/// somehow doesn't have an entry for it.
fn active_language_native(active_language: &Arc<Mutex<LanguageCode>>) -> String {
    let code = read_active_language(active_language);
    crate::lang::registry::enabled()
        .iter()
        .find(|l| l.code == code)
        .map(|l| l.native.clone())
        .unwrap_or_else(|| format!("{code:?}"))
}

#[cfg(target_os = "macos")]
fn show_pill(app: &AppHandle, mode: Mode, active_language: &Arc<Mutex<LanguageCode>>) {
    let label = match mode {
        Mode::Dictate => active_language_native(active_language),
        Mode::Command => "Say a language…".to_string(),
    };
    crate::overlay::panel::emit_show(app, mode, &label);
}

#[cfg(not(target_os = "macos"))]
fn show_pill(app: &AppHandle, mode: Mode, active_language: &Arc<Mutex<LanguageCode>>) {
    let label = match mode {
        Mode::Dictate => active_language_native(active_language),
        Mode::Command => "Say a language\u{2026}".to_string(),
    };
    crate::overlay::pill_windows::emit_show(app, mode, &label);
}

fn handle_raw_event(
    raw: RawKeyEvent,
    machine: &mut ChordMachine,
    arm_deadline: &mut Option<Instant>,
    audio: &mut AudioController,
    active_language: &Arc<Mutex<LanguageCode>>,
    app: &AppHandle,
    listening_enabled: &AtomicBool,
) {
    match raw {
        RawKeyEvent::Down(HotKey::RightCommand) => {
            if !listening_enabled.load(Ordering::Relaxed) {
                return;
            }
            if let ChordOutcome::ArmStarted =
                machine.on_event(ChordEvent::RightCommandDown { at_ms: now_ms() })
            {
                info!("right command down — arming (begin buffering audio now)");
                audio.start();
                *arm_deadline = Some(Instant::now() + Duration::from_millis(ARM_WINDOW_MS));
            }
        }
        RawKeyEvent::Up(HotKey::RightCommand) => {
            *arm_deadline = None;
            let prior_phase = machine.phase();
            match machine.on_event(ChordEvent::RightCommandUp { at_ms: now_ms() }) {
                ChordOutcome::AccidentalTap => {
                    info!("right command released before arming resolved — accidental tap, discarded");
                    audio.discard();
                }
                ChordOutcome::Idle => handle_session_end(prior_phase, audio, active_language, app),
                _ => {}
            }
        }
        RawKeyEvent::Down(HotKey::RightOption) => {
            machine.on_event(ChordEvent::RightOptionDown);
        }
        RawKeyEvent::Up(HotKey::RightOption) => {
            machine.on_event(ChordEvent::RightOptionUp);
        }
        RawKeyEvent::Down(HotKey::Escape) => {
            if let ChordOutcome::Cancelled = machine.on_event(ChordEvent::Escape) {
                info!("Cancelled");
                audio.discard();
                #[cfg(target_os = "macos")]
                crate::overlay::panel::emit_cancelled(app);
                #[cfg(not(target_os = "macos"))]
                crate::overlay::pill_windows::emit_cancelled(app);
            }
        }
        RawKeyEvent::Up(HotKey::Escape) => {}
    }
}

fn handle_session_end(
    prior_phase: Phase,
    audio: &mut AudioController,
    active_language: &Arc<Mutex<LanguageCode>>,
    app: &AppHandle,
) {
    match prior_phase {
        Phase::Recording(Mode::Dictate) => {
            if let Some(samples) = audio.finish() {
                #[cfg(target_os = "macos")]
                crate::overlay::panel::emit_transcribing(app);
                #[cfg(not(target_os = "macos"))]
                crate::overlay::pill_windows::emit_transcribing(app);
                spawn_dictate_pipeline(samples, active_language.clone(), app.clone());
            }
        }
        Phase::Recording(Mode::Command) => {
            if let Some(samples) = audio.finish() {
                #[cfg(target_os = "macos")]
                crate::overlay::panel::emit_transcribing(app);
                #[cfg(not(target_os = "macos"))]
                crate::overlay::pill_windows::emit_transcribing(app);
                spawn_command_pipeline(samples, active_language.clone(), app.clone());
            }
        }
        Phase::Cancelled => {
            // Audio was already discarded, and the pill already faded,
            // when Esc fired.
            info!("session ended — back to idle");
        }
        Phase::Idle | Phase::Arming => {}
    }
}

#[cfg(target_os = "macos")]
fn hide_pill(app: &AppHandle) {
    crate::overlay::panel::emit_hide(app);
}

#[cfg(not(target_os = "macos"))]
fn hide_pill(app: &AppHandle) {
    crate::overlay::pill_windows::emit_hide(app);
}

/// Runs ASR + injection on a blocking thread so the hotkey loop stays
/// responsive to the next key-down while a transcription is in flight.
fn spawn_dictate_pipeline(samples: Vec<f32>, active_language: Arc<Mutex<LanguageCode>>, app: AppHandle) {
    tokio::task::spawn_blocking(move || {
        let language = read_active_language(&active_language);

        let engine = match crate::asr::manager::dictate_engine(language) {
            Ok(engine) => engine,
            Err(err) => {
                tracing::error!(?err, "ASR unavailable");
                hide_pill(&app);
                return;
            }
        };

        use crate::asr::engine::AsrOptions;
        let opts = AsrOptions {
            language_hint: Some(language),
        };
        match engine.transcribe(&samples, &opts) {
            Ok(transcript) => {
                info!(
                    text = %transcript.text,
                    ms = transcript.duration_ms,
                    language = ?transcript.language,
                    engine = engine.id(),
                    "transcribed"
                );
                if is_only_bracketed_noise(&transcript.text) {
                    info!(text = %transcript.text, "discarding hallucinated non-speech transcript");
                } else if let Err(err) = crate::inject::inject_text(&transcript.text) {
                    tracing::error!(?err, "text injection failed");
                }
            }
            Err(err) => tracing::warn!(?err, "transcription produced nothing"),
        }
        hide_pill(&app);
    });
}

/// Whisper hallucinates bracketed non-speech labels (`[Music]`,
/// `[Applause]`, `[BLANK_AUDIO]`, ...) instead of returning nothing when
/// the recorded audio is near-silent or just ambient noise — e.g. an
/// accidental Right Command tap-and-release that's long enough to escape
/// the arm window's `AccidentalTap` discard, but catches no real speech.
/// True when `text` is made up of nothing but one or more such bracketed
/// tokens, with no real content outside them — that's never something
/// worth injecting into whatever app has focus.
fn is_only_bracketed_noise(text: &str) -> bool {
    let mut outside_brackets = String::new();
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => outside_brackets.push(c),
            _ => {}
        }
    }
    !text.trim().is_empty() && outside_brackets.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_bracketed_only_hallucinations() {
        assert!(is_only_bracketed_noise("[Music]"));
        assert!(is_only_bracketed_noise("[Music][Music][Band Warms Up][Music][Bell]"));
        assert!(is_only_bracketed_noise("  [Applause]  "));
    }

    #[test]
    fn keeps_real_speech_even_with_brackets_in_it() {
        assert!(!is_only_bracketed_noise("turn on the lights [laughs]"));
        assert!(!is_only_bracketed_noise("hello world"));
    }

    #[test]
    fn empty_text_is_not_flagged_as_noise() {
        // Handled separately upstream (Err(AsrError::Empty)) — this just
        // documents that an empty string isn't itself "bracketed noise".
        assert!(!is_only_bracketed_noise(""));
        assert!(!is_only_bracketed_noise("   "));
    }
}

/// Runs ASR + the fuzzy language matcher on a blocking thread, then
/// updates `active_language` if a language was heard clearly enough
/// (PROMPT.md §8).
fn spawn_command_pipeline(samples: Vec<f32>, active_language: Arc<Mutex<LanguageCode>>, app: AppHandle) {
    tokio::task::spawn_blocking(move || {
        let engine = match crate::asr::manager::command_engine() {
            Ok(engine) => engine,
            Err(err) => {
                tracing::error!(?err, "command-mode ASR unavailable");
                hide_pill(&app);
                return;
            }
        };

        use crate::asr::engine::{AsrEngine, AsrOptions};
        // TODO: bias decoding toward enabled languages' names for free
        // accuracy (PROMPT.md §8) — needs sherpa-onnx's hotwords_file,
        // which is set at recognizer-creation time, not per-utterance
        // like this options struct; not wired up yet.
        let opts = AsrOptions {
            language_hint: Some(LanguageCode::En),
        };

        let transcript = match engine.transcribe(&samples, &opts) {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(?err, "didn't catch anything in command mode");
                hide_pill(&app);
                return;
            }
        };
        info!(text = %transcript.text, "command heard");

        use crate::lang::matcher::{match_language, MatchResult};
        match match_language(&transcript.text, crate::lang::registry::enabled()) {
            MatchResult::Matched(code, score) => {
                let mut guard = active_language.lock().unwrap_or_else(|e| e.into_inner());
                *guard = code;
                drop(guard);
                info!(?code, score, "language switched");
                let native = crate::lang::registry::enabled()
                    .iter()
                    .find(|l| l.code == code)
                    .map(|l| l.native.clone())
                    .unwrap_or_else(|| format!("{code:?}"));
                #[cfg(target_os = "macos")]
                crate::overlay::panel::emit_matched(&app, &native);
                #[cfg(not(target_os = "macos"))]
                crate::overlay::pill_windows::emit_matched(&app, &native);
            }
            MatchResult::Ambiguous(a, b) => {
                info!(?a, ?b, "ambiguous language match — no picker UI yet (M6 pill work), keeping current language");
                hide_pill(&app);
            }
            MatchResult::NoMatch => {
                info!(heard = %transcript.text, "didn't recognize a language name — keeping current language");
                hide_pill(&app);
            }
        }
    });
}

fn log_mode(mode: Mode) {
    match mode {
        Mode::Dictate => info!("Dictate"),
        Mode::Command => info!("Command"),
    }
}
