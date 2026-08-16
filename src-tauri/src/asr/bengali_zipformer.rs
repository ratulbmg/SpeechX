//! A real, Bengali-specific ASR model — `sherpa-onnx-streaming-zipformer-bn-vosk`,
//! trained by Vosk and published in sherpa-onnx's own model zoo. Not
//! AI4Bharat IndicConformer (nobody has published a working ONNX export
//! of that anywhere — see `asr::whisper`'s doc comment), but a genuine
//! accuracy improvement over routing Bengali through general-purpose
//! Whisper, and it required no ML export work: this file is on-disk and
//! ready to load as-is.
//!
//! It's a *streaming* transducer model (sherpa-onnx's "online" API,
//! architecturally different from the "offline" API `WhisperEngine`
//! uses), because that's the only pre-built Bengali model that exists —
//! there's no offline/batch Bengali model in the zoo. We don't do real
//! incremental streaming with it: `transcribe` feeds the whole buffered
//! recording in one shot, calls `input_finished()`, drains every ready
//! decode step, then reads the final result. That's a standard, supported
//! way to run a streaming model over a complete pre-recorded clip.

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineTransducerModelConfig};
use tracing::{info, warn};

use super::engine::{AsrEngine, AsrError, AsrOptions, LanguageCode, Transcript};

pub struct BengaliZipformerEngine {
    recognizer: Mutex<OnlineRecognizer>,
}

impl BengaliZipformerEngine {
    pub fn load(encoder: &Path, decoder: &Path, joiner: &Path, tokens: &Path) -> Result<Self, AsrError> {
        for path in [encoder, decoder, joiner, tokens] {
            if !path.exists() {
                return Err(AsrError::ModelMissing(path.display().to_string()));
            }
        }

        let mut config = OnlineRecognizerConfig::default();
        config.model_config.transducer = OnlineTransducerModelConfig {
            encoder: Some(encoder.display().to_string()),
            decoder: Some(decoder.display().to_string()),
            joiner: Some(joiner.display().to_string()),
        };
        config.model_config.tokens = Some(tokens.display().to_string());
        config.model_config.num_threads = 2;
        config.model_config.provider = Some("cpu".into());
        config.decoding_method = Some("greedy_search".into());

        let recognizer = OnlineRecognizer::create(&config).ok_or(AsrError::RecognizerInit)?;
        info!("Bengali streaming Zipformer model loaded");
        Ok(Self {
            recognizer: Mutex::new(recognizer),
        })
    }
}

impl AsrEngine for BengaliZipformerEngine {
    fn id(&self) -> &str {
        "zipformer-bn-vosk"
    }

    fn supports(&self, lang: LanguageCode) -> bool {
        lang == LanguageCode::Bn
    }

    fn transcribe(&self, samples: &[f32], opts: &AsrOptions) -> Result<Transcript, AsrError> {
        let started = Instant::now();
        let recognizer = self.recognizer.lock().map_err(|_| AsrError::Poisoned)?;

        let stream = recognizer.create_stream();
        stream.accept_waveform(16_000, samples);
        stream.input_finished();
        while recognizer.is_ready(&stream) {
            recognizer.decode(&stream);
        }

        let text = recognizer
            .get_result(&stream)
            .ok_or(AsrError::Empty)?
            .text
            .trim()
            .to_string();

        if text.is_empty() {
            return Err(AsrError::Empty);
        }

        Ok(Transcript {
            text,
            language: opts.language_hint.unwrap_or(LanguageCode::Bn),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn memory_estimate_mb(&self) -> usize {
        100
    }

    fn warmup(&self) -> Result<(), AsrError> {
        let silence = vec![0.0_f32; 16_000]; // 1s
        if let Err(err) = self.transcribe(&silence, &AsrOptions::default()) {
            warn!(?err, "warmup transcription failed (non-fatal — silence often decodes to nothing)");
        }
        Ok(())
    }
}
