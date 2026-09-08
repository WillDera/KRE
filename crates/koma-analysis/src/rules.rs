//! Deterministic lexicon-based narrative analyzer.

use std::collections::BTreeSet;

use koma_core::kir::Document;

use crate::result::{
    ANALYSIS_VERSION, AnalysisResult, ChapterAnalysis, DocumentHints, EntityRef, LocationRef,
};
use crate::NarrativeAnalyzer;

/// Rule-based analyzer: fixed lexicons, word-boundary matching, sorted
/// outputs. Always available; no network or models.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedAnalyzer;

impl NarrativeAnalyzer for RuleBasedAnalyzer {
    fn id(&self) -> &'static str {
        "koma-analysis-rules"
    }

    fn analyze(&self, document: &Document) -> AnalysisResult {
        let seed = document.metadata.as_ref().map(|m| m.seed).unwrap_or(0);
        let mut chapters = Vec::with_capacity(document.chapters.len());
        for chapter in &document.chapters {
            chapters.push(analyze_text(&chapter.id, &chapter.plain_text()));
        }

        let document_hints = aggregate_document(&chapters);
        AnalysisResult {
            version: ANALYSIS_VERSION.to_owned(),
            generator: self.id().to_owned(),
            seed,
            document: document_hints,
            chapters,
        }
    }
}

/// Mood rules: (needle, mood). First match wins; more specific needles first.
const MOOD_RULES: &[(&str, &str)] = &[
    ("frost", "ominous"),
    ("frozen", "ominous"),
    ("blizzard", "ominous"),
    ("snow", "ominous"),
    ("ice", "ominous"),
    ("cold", "ominous"),
    ("ominous", "ominous"),
    ("shadow", "dark"),
    ("night", "dark"),
    ("dark", "dark"),
    ("gloom", "dark"),
    ("sun", "warm"),
    ("warm", "warm"),
    ("joy", "warm"),
    ("hope", "warm"),
];

/// Environment rules: (needle, environment label). First match wins.
const ENV_RULES: &[(&str, &str)] = &[
    ("spaceship", "space"),
    ("orbit", "space"),
    ("starship", "space"),
    ("nebula", "space"),
    ("space", "space"),
    ("stars", "space"),
    ("chamber", "indoor"),
    ("archive", "indoor"),
    ("library", "indoor"),
    ("hall", "indoor"),
    ("room", "indoor"),
    ("castle", "indoor"),
    ("forest", "outdoor"),
    ("woods", "outdoor"),
    ("mountain", "outdoor"),
    ("field", "outdoor"),
    ("frozen", "frozen"),
    ("tundra", "frozen"),
    ("blizzard", "frozen"),
    ("snow", "frozen"),
];

/// Theme tags collected when any needle matches.
const THEME_RULES: &[(&str, &str)] = &[
    ("empire", "empire"),
    ("imperial", "empire"),
    ("war", "conflict"),
    ("battle", "conflict"),
    ("love", "romance"),
    ("winter", "winter"),
    ("snow", "winter"),
    ("frost", "winter"),
    ("magic", "magic"),
    ("spell", "magic"),
    ("machine", "technology"),
    ("engine", "technology"),
    ("inquisitor", "authority"),
    ("emperor", "authority"),
];

/// Role entities (needle → display name).
const ENTITY_RULES: &[(&str, &str)] = &[
    ("inquisitor", "Inquisitor"),
    ("emperor", "Emperor"),
    ("captain", "Captain"),
    ("navigator", "Navigator"),
    ("archivist", "Archivist"),
];

/// Place locations (needle → display name).
const LOCATION_RULES: &[(&str, &str)] = &[
    ("chamber", "Chamber"),
    ("archive", "Archive"),
    ("library", "Library"),
    ("forest", "Forest"),
    ("hubris", "Hubris Planet"),
];

fn analyze_text(chapter_id: &str, text: &str) -> ChapterAnalysis {
    let lower = text.to_ascii_lowercase();
    ChapterAnalysis {
        chapter_id: chapter_id.to_owned(),
        mood: first_match(&lower, MOOD_RULES).map(str::to_owned),
        environment: first_match(&lower, ENV_RULES).map(str::to_owned),
        themes: collect_unique(&lower, THEME_RULES),
        entities: collect_entities(&lower),
        locations: collect_locations(&lower),
    }
}

fn first_match<'a>(haystack: &str, rules: &'a [(&str, &str)]) -> Option<&'a str> {
    for (needle, value) in rules {
        if contains_word(haystack, needle) {
            return Some(*value);
        }
    }
    None
}

fn collect_unique(haystack: &str, rules: &[(&str, &str)]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for (needle, tag) in rules {
        if contains_word(haystack, needle) {
            set.insert((*tag).to_owned());
        }
    }
    set.into_iter().collect()
}

fn collect_entities(haystack: &str) -> Vec<EntityRef> {
    let mut set = BTreeSet::new();
    for (needle, name) in ENTITY_RULES {
        if contains_word(haystack, needle) {
            set.insert(EntityRef {
                kind: "character".to_owned(),
                name: (*name).to_owned(),
            });
        }
    }
    set.into_iter().collect()
}

fn collect_locations(haystack: &str) -> Vec<LocationRef> {
    let mut set = BTreeSet::new();
    for (needle, name) in LOCATION_RULES {
        if contains_word(haystack, needle) {
            set.insert(LocationRef {
                kind: "place".to_owned(),
                name: (*name).to_owned(),
            });
        }
    }
    set.into_iter().collect()
}

/// ASCII word-boundary substring check (deterministic, locale-light).
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= bytes.len() {
        if &bytes[i..i + n.len()] == n {
            let before_ok = i == 0 || !is_ascii_word_byte(bytes[i - 1]);
            let after = i + n.len();
            let after_ok = after == bytes.len() || !is_ascii_word_byte(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ascii_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn aggregate_document(chapters: &[ChapterAnalysis]) -> DocumentHints {
    let mut themes = BTreeSet::new();
    let mut mood = None;
    let mut environment = None;
    for ch in chapters {
        if mood.is_none() {
            mood = ch.mood.clone();
        }
        if environment.is_none() {
            environment = ch.environment.clone();
        }
        for t in &ch.themes {
            themes.insert(t.clone());
        }
    }
    DocumentHints {
        mood,
        environment,
        themes: themes.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use koma_core::kir::{Block, Chapter, Section, block};

    use super::*;
    use crate::NarrativeAnalyzer;

    fn doc_with(text: &str) -> Document {
        let mut doc = Document::new();
        doc.metadata = Some(koma_core::kir::DocumentMetadata {
            title: Some("t".into()),
            author: None,
            language: Some("en".into()),
            identifiers: vec![],
            license: None,
            generator: "test".into(),
            seed: 7,
        });
        doc.chapters.push(Chapter {
            id: "ch1".into(),
            title: Some("One".into()),
            sections: vec![Section {
                id: "s1".into(),
                blocks: vec![Block {
                    kind: Some(block::Kind::Paragraph(koma_core::kir::paragraph(text))),
                }],
            }],
        });
        doc
    }

    #[test]
    fn detects_ominous_frozen_chamber() {
        let result = RuleBasedAnalyzer.analyze(&doc_with(
            "A cold coming. The old inquisitor entered the chamber under frost.",
        ));
        let ch = result.chapter("ch1").unwrap();
        assert_eq!(ch.mood.as_deref(), Some("ominous"));
        assert_eq!(ch.environment.as_deref(), Some("indoor"));
        assert!(ch.themes.iter().any(|t| t == "winter" || t == "authority"));
        assert!(
            ch.entities
                .iter()
                .any(|e| e.name == "Inquisitor" && e.kind == "character")
        );
        assert!(ch.locations.iter().any(|l| l.name == "Chamber"));
        assert_eq!(result.seed, 7);
        assert_eq!(result.generator, "koma-analysis-rules");
    }

    #[test]
    fn detects_space_environment() {
        let result = RuleBasedAnalyzer.analyze(&doc_with("The starship left orbit for deep space."));
        let ch = result.chapter("ch1").unwrap();
        assert_eq!(ch.environment.as_deref(), Some("space"));
    }

    #[test]
    fn analysis_is_deterministic_and_serializable() {
        let doc = doc_with("Snow fell across the frozen archive.");
        let a = RuleBasedAnalyzer.analyze(&doc);
        let b = RuleBasedAnalyzer.analyze(&doc);
        assert_eq!(a, b);
        let json = a.to_json().unwrap();
        let round = AnalysisResult::from_json(json.as_bytes()).unwrap();
        assert_eq!(round, a);
    }

    #[test]
    fn word_boundary_avoids_partial_hits() {
        // "cold" must not match inside "scolding"
        assert!(!contains_word("scolding the crew", "cold"));
        assert!(contains_word("a cold night", "cold"));
    }
}
