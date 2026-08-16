//! The `AsrEngine` abstraction (PROMPT.md §7) — lets the session layer
//! transcribe without knowing whether the samples went to Whisper or
//! (once exported) IndicConformer.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageCode {
    En,
    Hi,
    Bn,
}

impl LanguageCode {
    pub fn whisper_code(self) -> &'static str {
        match self {
            LanguageCode::En => "en",
            LanguageCode::Hi => "hi",
            LanguageCode::Bn => "bn",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: LanguageCode,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AsrOptions {
    pub language_hint: Option<LanguageCode>,
}

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("model files missing: {0}")]
    ModelMissing(String),
    #[error("failed to create recognizer")]
    RecognizerInit,
    #[error("asr engine mutex poisoned by a prior panic")]
    Poisoned,
    #[error("transcription produced no result")]
    Empty,
}

pub trait AsrEngine: Send + Sync {
    fn id(&self) -> &str;

    /// Part of the trait contract (PROMPT.md §7) but no call site yet —
    /// `asr::manager` currently routes by `LanguageCode` match rather
    /// than by asking each engine what it supports. Will matter once
    /// there's more than one candidate engine per language to choose
    /// between.
    #[allow(dead_code)]
    fn supports(&self, lang: LanguageCode) -> bool;

    /// `samples`: 16 kHz, mono, f32 normalised to [-1.0, 1.0].
    fn transcribe(&self, samples: &[f32], opts: &AsrOptions) -> Result<Transcript, AsrError>;

    /// Approximate resident memory. No consumer yet — PROMPT.md §7's
    /// idle-eviction manager (M7/M8) is what would sum these against a
    /// memory budget.
    #[allow(dead_code)]
    fn memory_estimate_mb(&self) -> usize;

    /// Run one throwaway inference so the first real one isn't slower.
    fn warmup(&self) -> Result<(), AsrError>;
}
