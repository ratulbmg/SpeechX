pub mod capture;
pub mod resample;
pub mod ring_buffer;

use std::time::{Duration, Instant};

use tauri::AppHandle;
use tracing::{error, info, warn};

use capture::CaptureSession;

const DEBUG_WAV_PATH: &str = "/tmp/speechx_test.wav";

/// PROMPT.md §10: "Throttle to ~30 Hz. Emitting per-buffer floods the IPC
/// channel and gains nothing visible."
const LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(33);

/// Owns the (at most one — the chord machine enforces this) in-flight
/// recording session.
pub struct AudioController {
    session: Option<CaptureSession>,
    app: AppHandle,
}

impl AudioController {
    pub fn new(app: AppHandle) -> Self {
        Self { session: None, app }
    }

    /// Begin buffering immediately — called on `RightCommand` key-down,
    /// before the mode (Dictate vs Command) is even known.
    pub fn start(&mut self) {
        if self.session.is_some() {
            warn!("audio capture already running — ignoring duplicate start");
            return;
        }
        match CaptureSession::start() {
            Ok(mut session) => {
                info!(
                    sample_rate = session.info.sample_rate,
                    channels = session.info.channels,
                    "audio capture started"
                );
                spawn_level_forwarder(session.take_level_rx(), self.app.clone());
                self.session = Some(session);
            }
            Err(err) => error!(%err, "failed to start audio capture"),
        }
    }

    /// Stop and drop everything captured — used on Cancelled/AccidentalTap.
    pub fn discard(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.stop();
            info!("audio capture discarded");
        }
    }

    /// Stop and return 16 kHz mono f32 samples ready for ASR. Also writes
    /// `/tmp/speechx_test.wav` at the device's native rate (PROMPT.md
    /// M2 "done when": the WAV plays back correctly, no clipped first
    /// syllable).
    pub fn finish(&mut self) -> Option<Vec<f32>> {
        let session = self.session.take()?;
        let (info, raw) = session.stop();
        info!(
            raw_samples = raw.len(),
            sample_rate = info.sample_rate,
            channels = info.channels,
            "audio capture finished"
        );

        if let Err(err) = write_debug_wav(DEBUG_WAV_PATH, &raw, info.sample_rate, info.channels) {
            warn!(?err, "failed to write debug wav");
        }

        Some(resample::to_asr_input(&raw, info.channels, info.sample_rate))
    }
}

/// Briefly opens then immediately closes an input stream, purely to
/// trigger the OS microphone-permission prompt at launch instead of
/// waiting for the user's first real hotkey press — that's a confusing
/// moment for a system dialog to interrupt mid-recording. Mirrors what
/// `permissions::macos::ensure_accessibility`/`ensure_input_monitoring`
/// already do for their own permissions, so all three prompts land
/// together right after install instead of trickling in over first use.
/// Non-fatal if it fails (no mic, denied, or — on Windows — the privacy
/// toggle being off): the real error still surfaces on first actual use,
/// this is just trying to get ahead of it.
pub fn warm_up_microphone_permission() {
    match CaptureSession::start() {
        Ok(session) => {
            let _ = session.stop();
            info!("microphone permission warmup: input stream opened and closed cleanly");
        }
        Err(err) => warn!(%err, "microphone permission warmup failed (non-fatal — will retry on first real use)"),
    }
}

fn spawn_level_forwarder(level_rx: std::sync::mpsc::Receiver<f32>, app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let mut last_emit = Instant::now() - LEVEL_EMIT_INTERVAL;
        while let Ok(level) = level_rx.recv() {
            let now = Instant::now();
            if now.duration_since(last_emit) < LEVEL_EMIT_INTERVAL {
                continue;
            }
            last_emit = now;
            #[cfg(target_os = "macos")]
            crate::overlay::panel::emit_audio_level(&app, level);
            #[cfg(not(target_os = "macos"))]
            let _ = (&app, level);
        }
    });
}

fn write_debug_wav(path: &str, raw_samples: &[f32], sample_rate: u32, channels: u16) -> std::io::Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(hound_err)?;
    for &sample in raw_samples {
        writer.write_sample(sample).map_err(hound_err)?;
    }
    writer.finalize().map_err(hound_err)
}

fn hound_err(e: hound::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}
