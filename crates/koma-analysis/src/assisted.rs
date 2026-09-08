//! Assisted compilation front door — same trait, rules fallback.
//!
//! External models are not wired in this phase. Any future assisted backend
//! must fall back to [`RuleBasedAnalyzer`] on error, denial, or absence
//! (AGENTS.md failure handling). The runtime never calls this path.

use koma_core::kir::Document;

use crate::result::AnalysisResult;
use crate::rules::RuleBasedAnalyzer;
use crate::NarrativeAnalyzer;

/// Compile-time assistance entry point. Phase 11 always delegates to the
/// rule-based analyzer so packages stay deterministic and offline-safe.
#[derive(Debug, Default, Clone, Copy)]
pub struct AssistedAnalyzer {
    rules: RuleBasedAnalyzer,
}

impl AssistedAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NarrativeAnalyzer for AssistedAnalyzer {
    fn id(&self) -> &'static str {
        "koma-analysis-assisted"
    }

    fn analyze(&self, document: &Document) -> AnalysisResult {
        let mut result = self.rules.analyze(document);
        // Preserve the assisted generator id so packages record which front
        // door was used; the semantic payload remains rule-derived.
        result.generator = self.id().to_owned();
        result
    }
}

#[cfg(test)]
mod tests {
    use koma_core::kir::{Block, Chapter, Section, block};

    use super::*;
    use crate::NarrativeAnalyzer;

    #[test]
    fn assisted_matches_rules_payload() {
        let mut doc = Document::new();
        doc.chapters.push(Chapter {
            id: "c".into(),
            title: None,
            sections: vec![Section {
                id: "s".into(),
                blocks: vec![Block {
                    kind: Some(block::Kind::Paragraph(koma_core::kir::paragraph(
                        "Snow in the chamber.",
                    ))),
                }],
            }],
        });
        let rules = RuleBasedAnalyzer.analyze(&doc);
        let assisted = AssistedAnalyzer::new().analyze(&doc);
        assert_eq!(assisted.chapters, rules.chapters);
        assert_eq!(assisted.generator, "koma-analysis-assisted");
    }
}
