//! Whisper base (multilingual) via the official `sherpa-onnx` crate.
//!
//! This is the only engine implemented so far. PROMPT.md's IndicConformer
//! path (`asr/conformer.rs`) needs AI4Bharat's NeMo checkpoints exported
//! to ONNX first (`scripts/export_indicconformer.py`, not written yet) —
//! there is no pre-packaged sherpa-onnx build of it to download. Whisper
//! base is multilingual and covers English/Hindi/Bengali zero-shot in the
//! meantime, just with lower Hindi/Bengali accuracy than a dedicated
//! fine-tuned model would give.

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineWhisperModelConfig};
use tracing::{info, warn};

use super::engine::{AsrEngine, AsrError, AsrOptions, LanguageCode, Transcript};

pub struct WhisperEngine {
    // SpeechX only ever runs one dictation session at a time (the chord
    // machine enforces that), so this mutex is never real contention —
    // it just lets `&self` methods create/decode a stream safely.
    recognizer: Mutex<OfflineRecognizer>,
    loaded_language: LanguageCode,
}

impl WhisperEngine {
    /// `language` is baked into the recognizer at load time — sherpa-onnx's
    /// greedy-search Whisper decoder doesn't take it per-utterance (and
    /// rejects "auto" outright, which is what crashed the process the
    /// first time this was wired up). Switching languages means loading a
    /// new `WhisperEngine`, not reconfiguring an existing one — cheap
    /// since it's the same on-disk model files each time, just a fresh
    /// `OfflineRecognizer::create` call (see `asr::manager::dictate_engine`).
    pub fn load(encoder: &Path, decoder: &Path, tokens: &Path, language: LanguageCode) -> Result<Self, AsrError> {
        for path in [encoder, decoder, tokens] {
            if !path.exists() {
                return Err(AsrError::ModelMissing(path.display().to_string()));
            }
        }

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.whisper = OfflineWhisperModelConfig {
            encoder: Some(encoder.display().to_string()),
            decoder: Some(decoder.display().to_string()),
            language: Some(language.whisper_code().to_string()),
            task: Some("transcribe".into()),
            ..Default::default()
        };
        config.model_config.tokens = Some(tokens.display().to_string());
        config.model_config.num_threads = 2;
        config.model_config.provider = Some("cpu".into());
        config.decoding_method = Some("greedy_search".into());

        let recognizer = OfflineRecognizer::create(&config).ok_or(AsrError::RecognizerInit)?;
        info!(?language, "whisper base model loaded");
        Ok(Self {
            recognizer: Mutex::new(recognizer),
            loaded_language: language,
        })
    }
}

impl AsrEngine for WhisperEngine {
    fn id(&self) -> &str {
        "whisper-base"
    }

    fn supports(&self, lang: LanguageCode) -> bool {
        lang == self.loaded_language
    }

    fn transcribe(&self, samples: &[f32], opts: &AsrOptions) -> Result<Transcript, AsrError> {
        let started = Instant::now();
        let recognizer = self.recognizer.lock().map_err(|_| AsrError::Poisoned)?;

        let stream = recognizer.create_stream();
        stream.accept_waveform(16_000, samples);
        recognizer.decode(&stream);

        let text = stream
            .get_result()
            .ok_or(AsrError::Empty)?
            .text
            .trim()
            .to_string();

        if text.is_empty() {
            return Err(AsrError::Empty);
        }

        Ok(Transcript {
            text,
            language: opts.language_hint.unwrap_or(self.loaded_language),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn memory_estimate_mb(&self) -> usize {
        120
    }

    fn warmup(&self) -> Result<(), AsrError> {
        let silence = vec![0.0_f32; 16_000]; // 1s
        if let Err(err) = self.transcribe(&silence, &AsrOptions::default()) {
            warn!(?err, "warmup transcription failed (non-fatal — silence often decodes to nothing)");
        }
        Ok(())
    }
}
