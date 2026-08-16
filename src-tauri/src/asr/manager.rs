//! Routes each language to whichever engine actually serves it:
//!
//! - `command_engine`: always English, always resident, used only to
//!   recognize spoken language names in Command mode. Loaded once and
//!   never swapped, so switching languages doesn't also break the ability
//!   to hear the *next* switch command.
//! - `dictate_engine(En | Hi)`: Whisper, reloaded (not downloaded — same
//!   files, just a fresh `OfflineRecognizer::create` with a different
//!   `language` baked in) whenever the active language changes between
//!   these two. At most one resident at a time.
//! - `dictate_engine(Bn)`: a dedicated, always-resident Bengali-specific
//!   model (`asr::bengali_zipformer`) — real accuracy improvement over
//!   routing Bengali through Whisper, and unlike Hindi it didn't require
//!   any ML export work to get.
//!
//! PROMPT.md's fuller manager — memory-mapped weights, idle eviction —
//! is still out of scope; today's priority was making Bengali actually
//! good, not squeezing idle RSS.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tracing::info;

use super::bengali_zipformer::BengaliZipformerEngine;
use super::engine::{AsrEngine, AsrError, LanguageCode};
use super::whisper::WhisperEngine;
use crate::config::paths;

fn whisper_model_paths() -> (PathBuf, PathBuf, PathBuf) {
    let dir = paths::model_dir().join("sherpa-onnx-whisper-base");
    (
        dir.join("base-encoder.int8.onnx"),
        dir.join("base-decoder.int8.onnx"),
        dir.join("base-tokens.txt"),
    )
}

fn load_whisper(language: LanguageCode) -> Result<WhisperEngine, String> {
    let (encoder, decoder, tokens) = whisper_model_paths();
    let engine = WhisperEngine::load(&encoder, &decoder, &tokens, language).map_err(|e| e.to_string())?;
    if let Err(err) = engine.warmup() {
        tracing::warn!(?err, "whisper warmup failed (non-fatal)");
    }
    info!(?language, "whisper ready");
    Ok(engine)
}

static COMMAND: OnceLock<Result<WhisperEngine, String>> = OnceLock::new();

pub fn command_engine() -> Result<&'static WhisperEngine, AsrError> {
    let result = COMMAND.get_or_init(|| load_whisper(LanguageCode::En));
    result.as_ref().map_err(|e| AsrError::ModelMissing(e.clone()))
}

static WHISPER_DICTATE: Mutex<Option<(LanguageCode, Arc<WhisperEngine>)>> = Mutex::new(None);

fn whisper_dictate_engine(language: LanguageCode) -> Result<Arc<WhisperEngine>, AsrError> {
    let mut guard = WHISPER_DICTATE.lock().unwrap_or_else(|e| e.into_inner());

    if let Some((current, engine)) = guard.as_ref() {
        if *current == language {
            return Ok(engine.clone());
        }
    }

    let engine = Arc::new(load_whisper(language).map_err(AsrError::ModelMissing)?);
    *guard = Some((language, engine.clone()));
    Ok(engine)
}

static BENGALI: OnceLock<Result<Arc<BengaliZipformerEngine>, String>> = OnceLock::new();

fn bengali_engine() -> Result<Arc<BengaliZipformerEngine>, AsrError> {
    let result = BENGALI.get_or_init(|| {
        let dir = paths::model_dir().join("sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09");
        let engine = BengaliZipformerEngine::load(
            &dir.join("encoder.onnx"),
            &dir.join("decoder.onnx"),
            &dir.join("joiner.onnx"),
            &dir.join("tokens.txt"),
        )
        .map_err(|e| e.to_string())?;
        if let Err(err) = engine.warmup() {
            tracing::warn!(?err, "bengali zipformer warmup failed (non-fatal)");
        }
        info!("bengali zipformer ready");
        Ok(Arc::new(engine))
    });

    match result {
        Ok(engine) => Ok(engine.clone()),
        Err(e) => Err(AsrError::ModelMissing(e.clone())),
    }
}

/// The active dictation engine for `language`.
pub fn dictate_engine(language: LanguageCode) -> Result<Arc<dyn AsrEngine>, AsrError> {
    match language {
        LanguageCode::Bn => Ok(bengali_engine()? as Arc<dyn AsrEngine>),
        LanguageCode::En | LanguageCode::Hi => Ok(whisper_dictate_engine(language)? as Arc<dyn AsrEngine>),
    }
}
