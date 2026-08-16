pub mod chord;
pub mod keymap;
pub mod listener;

use std::future::pending;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
pub async fn run() {
    let mut rx = match listener::spawn_listener() {
        Ok(rx) => rx,
        Err(err) => {
            tracing::error!(?err, "failed to start hotkey listener");
            return;
        }
    };

    info!("hotkey listener attached — hold Right Cmd to dictate, Right Cmd + Right Option to switch language");

    let mut machine = ChordMachine::new();
    let mut audio = AudioController::new();
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
                }
            }

            maybe_event = rx.recv() => {
                let Some(raw) = maybe_event else {
                    tracing::warn!("hotkey listener channel closed — stopping");
                    break;
                };
                handle_raw_event(raw, &mut machine, &mut arm_deadline, &mut audio, &active_language);
            }
        }
    }
}

fn handle_raw_event(
    raw: RawKeyEvent,
    machine: &mut ChordMachine,
    arm_deadline: &mut Option<Instant>,
    audio: &mut AudioController,
    active_language: &Arc<Mutex<LanguageCode>>,
) {
    match raw {
        RawKeyEvent::Down(HotKey::RightCommand) => {
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
                ChordOutcome::Idle => handle_session_end(prior_phase, audio, active_language),
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
            }
        }
        RawKeyEvent::Up(HotKey::Escape) => {}
    }
}

fn handle_session_end(prior_phase: Phase, audio: &mut AudioController, active_language: &Arc<Mutex<LanguageCode>>) {
    match prior_phase {
        Phase::Recording(Mode::Dictate) => {
            if let Some(samples) = audio.finish() {
                spawn_dictate_pipeline(samples, active_language.clone());
            }
        }
        Phase::Recording(Mode::Command) => {
            if let Some(samples) = audio.finish() {
                spawn_command_pipeline(samples, active_language.clone());
            }
        }
        Phase::Cancelled => {
            // Audio was already discarded when Esc fired.
            info!("session ended — back to idle");
        }
        Phase::Idle | Phase::Arming => {}
    }
}

/// Runs ASR + injection on a blocking thread so the hotkey loop stays
/// responsive to the next key-down while a transcription is in flight.
fn spawn_dictate_pipeline(samples: Vec<f32>, active_language: Arc<Mutex<LanguageCode>>) {
    tokio::task::spawn_blocking(move || {
        let language = read_active_language(&active_language);

        let engine = match crate::asr::manager::dictate_engine(language) {
            Ok(engine) => engine,
            Err(err) => {
                tracing::error!(?err, "ASR unavailable");
                return;
            }
        };

        use crate::asr::engine::AsrOptions;
        let opts = AsrOptions {
            language_hint: Some(language),
            ..Default::default()
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
                if let Err(err) = crate::inject::inject_text(&transcript.text) {
                    tracing::error!(?err, "text injection failed");
                }
            }
            Err(err) => tracing::warn!(?err, "transcription produced nothing"),
        }
    });
}

/// Runs ASR + the fuzzy language matcher on a blocking thread, then
/// updates `active_language` if a language was heard clearly enough
/// (PROMPT.md §8).
fn spawn_command_pipeline(samples: Vec<f32>, active_language: Arc<Mutex<LanguageCode>>) {
    tokio::task::spawn_blocking(move || {
        let engine = match crate::asr::manager::command_engine() {
            Ok(engine) => engine,
            Err(err) => {
                tracing::error!(?err, "command-mode ASR unavailable");
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
            }
            MatchResult::Ambiguous(a, b) => {
                info!(?a, ?b, "ambiguous language match — no picker UI yet (M5/M6 pill work), keeping current language");
            }
            MatchResult::NoMatch => {
                info!(heard = %transcript.text, "didn't recognize a language name — keeping current language");
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
