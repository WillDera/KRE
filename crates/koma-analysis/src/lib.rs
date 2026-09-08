//! koma-analysis: semantic understanding of KIR at compile time.
//!
//! Output is **compilation data only** (AGENTS.md). The runtime never calls
//! analyzers and never depends on external models. Rule-based analysis is
//! always available; assisted compilation uses the same
//! [`NarrativeAnalyzer`] trait and falls back to rules.

pub mod assisted;
pub mod result;
pub mod rules;

pub use assisted::AssistedAnalyzer;
pub use result::{
    ANALYSIS_VERSION, AnalysisError, AnalysisResult, ChapterAnalysis, DocumentHints, EntityRef,
    LocationRef,
};
pub use rules::RuleBasedAnalyzer;

use koma_core::kir::Document;

/// Semantic analyzer. Implementations must be deterministic given the same
/// document (and seed). External models are optional and compile-time only.
pub trait NarrativeAnalyzer {
    fn id(&self) -> &'static str;
    fn analyze(&self, document: &Document) -> AnalysisResult;
}
