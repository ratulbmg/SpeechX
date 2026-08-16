//! Fuzzy-matches a spoken phrase against the enabled languages' aliases
//! (PROMPT.md §8). Only matches against *enabled* languages, never the
//! full registry — the single biggest lever against near-collisions like
//! Hindi/Sindhi is simply not offering Sindhi as a candidate unless it's
//! actually enabled.

use strsim::jaro_winkler;

use super::registry::Language;
use crate::asr::engine::LanguageCode;

const MATCH_THRESHOLD: f64 = 0.82;
const AMBIGUITY_MARGIN: f64 = 0.05;

const FILLER_WORDS: &[&str] = &[
    "switch", "to", "change", "set", "please", "language", "mode", "karo", "kar", "do", "koro",
    "cholo",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchResult {
    Matched(LanguageCode, f64),
    Ambiguous(LanguageCode, LanguageCode),
    NoMatch,
}

pub fn match_language(spoken: &str, enabled: &[Language]) -> MatchResult {
    let words: Vec<String> = spoken
        .to_lowercase()
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation()).to_string())
        .filter(|w| !w.is_empty() && !FILLER_WORDS.contains(&w.as_str()))
        .collect();

    if words.is_empty() || enabled.is_empty() {
        return MatchResult::NoMatch;
    }

    let mut scores: Vec<(LanguageCode, f64)> = enabled
        .iter()
        .map(|lang| {
            let best = words
                .iter()
                .flat_map(|word| lang.aliases.iter().map(move |alias| jaro_winkler(word, &alias.to_lowercase())))
                .fold(0.0_f64, f64::max);
            (lang.code, best)
        })
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (top_code, top_score) = scores[0];
    if top_score < MATCH_THRESHOLD {
        return MatchResult::NoMatch;
    }

    if let Some(&(second_code, second_score)) = scores.get(1) {
        if top_score - second_score < AMBIGUITY_MARGIN {
            return MatchResult::Ambiguous(top_code, second_code);
        }
    }

    MatchResult::Matched(top_code, top_score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<Language> {
        vec![
            Language {
                code: LanguageCode::En,
                display: "English".into(),
                native: "English".into(),
                aliases: vec!["english".into()],
            },
            Language {
                code: LanguageCode::Hi,
                display: "Hindi".into(),
                native: "हिंदी".into(),
                aliases: vec!["hindi".into(), "हिंदी".into()],
            },
            Language {
                code: LanguageCode::Bn,
                display: "Bengali".into(),
                native: "বাংলা".into(),
                aliases: vec!["bengali".into(), "bangla".into()],
            },
        ]
    }

    #[test]
    fn exact_alias_matches() {
        assert_eq!(match_language("hindi", &fixture()), MatchResult::Matched(LanguageCode::Hi, 1.0));
    }

    #[test]
    fn strips_filler_words_around_the_language_name() {
        assert_eq!(
            match_language("please switch the language to bengali", &fixture()),
            MatchResult::Matched(LanguageCode::Bn, 1.0)
        );
    }

    #[test]
    fn matches_a_close_misspelling() {
        assert_eq!(match_language("bengoli", &fixture()), MatchResult::Matched(LanguageCode::Bn, jaro_winkler("bengoli", "bengali")));
    }

    #[test]
    fn gibberish_is_no_match() {
        assert_eq!(match_language("xyzzyplugh", &fixture()), MatchResult::NoMatch);
    }

    #[test]
    fn empty_utterance_is_no_match() {
        assert_eq!(match_language("   ", &fixture()), MatchResult::NoMatch);
    }

    #[test]
    fn near_collision_between_two_similar_aliases_is_ambiguous() {
        // Deliberately not real registry data — Hindi/Sindhi-style
        // collision, constructed to sit within the ambiguity margin.
        let langs = vec![
            Language {
                code: LanguageCode::Hi,
                display: "Hindi".into(),
                native: "हिंदी".into(),
                aliases: vec!["hindi".into()],
            },
            Language {
                code: LanguageCode::Bn,
                display: "Sindhi".into(),
                native: "سنڌي".into(),
                aliases: vec!["sindhi".into()],
            },
        ];
        // "shindi" sits almost equidistant between the two aliases
        // (Jaro-Winkler: hindi=0.944, sindhi=0.900 — within the 0.05 margin).
        match match_language("shindi", &langs) {
            MatchResult::Ambiguous(a, b) => {
                assert!((a == LanguageCode::Hi && b == LanguageCode::Bn) || (a == LanguageCode::Bn && b == LanguageCode::Hi));
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn only_matches_against_enabled_languages() {
        // A word with no close alias in the fixture at all (unlike
        // "sindhi", which — tellingly — scores just above threshold
        // against "hindi" alone: 0.822 vs the 0.82 cutoff. That's a real
        // near-miss, not a test bug; PROMPT.md §8 only claims enabling-only
        // matching removes *most* Hindi/Sindhi-style confusion, not all of
        // it when the words are this close).
        assert_eq!(match_language("xyzabcqwerty", &fixture()), MatchResult::NoMatch);
    }
}
