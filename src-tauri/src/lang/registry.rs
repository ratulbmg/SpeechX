//! Loads `languages.toml` (PROMPT.md §8). Embedded at compile time via
//! `include_str!` rather than read from disk at runtime — there's no
//! Tauri resource-bundling pipeline set up yet to ship a loose data file
//! alongside the binary, so for now "editing the data file" means editing
//! it and rebuilding, not editing an installed app's files.

use serde::Deserialize;

use crate::asr::engine::LanguageCode;

const LANGUAGES_TOML: &str = include_str!("languages.toml");

#[derive(Debug, Deserialize)]
struct RawFile {
    language: Vec<RawLanguage>,
}

#[derive(Debug, Deserialize)]
struct RawLanguage {
    code: String,
    display: String,
    native: String,
    aliases: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Language {
    pub code: LanguageCode,
    // No reader yet since command-mode hotwords biasing isn't wired up
    // (see hotkey::spawn_command_pipeline's TODO).
    #[allow(dead_code)]
    pub display: String,
    pub native: String,
    pub aliases: Vec<String>,
}

/// All languages enabled for matching. Everything in `languages.toml` is
/// enabled unconditionally today — a real enabled-set driven by
/// `settings.toml [languages] enabled` is M8 scope.
pub fn enabled() -> &'static [Language] {
    static REGISTRY: std::sync::OnceLock<Vec<Language>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        // `languages.toml` is our own file, embedded at compile time — a
        // parse failure here is a build-time authoring mistake, not a
        // runtime condition, so failing loudly is correct.
        let raw: RawFile = match toml::from_str(LANGUAGES_TOML) {
            Ok(raw) => raw,
            Err(err) => panic!("languages.toml is malformed: {err}"),
        };

        raw.language
            .into_iter()
            .filter_map(|l| {
                let code = match l.code.as_str() {
                    "en" => LanguageCode::En,
                    "hi" => LanguageCode::Hi,
                    "bn" => LanguageCode::Bn,
                    other => {
                        tracing::warn!(code = other, "unknown language code in languages.toml, skipping");
                        return None;
                    }
                };
                Some(Language {
                    code,
                    display: l.display,
                    native: l.native,
                    aliases: l.aliases,
                })
            })
            .collect()
    })
}
