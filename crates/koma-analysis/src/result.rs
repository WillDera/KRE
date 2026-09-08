//! Versioned analysis result — compilation data embedded in `.koma` packages.

use serde::{Deserialize, Serialize};

/// Analysis result schema version. Breaking changes require migration.
pub const ANALYSIS_VERSION: &str = "0.1.0";

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("unsupported analysis version `{0}` (supported: {1})")]
    UnsupportedVersion(String, &'static str),
    #[error("serialize error: {0}")]
    Serialize(String),
    #[error("deserialize error: {0}")]
    Deserialize(String),
}

/// Document-level aggregate hints.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<String>,
}

/// One chapter's semantic summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterAnalysis {
    pub chapter_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<LocationRef>,
}

/// Named entity (character, etc.). Mirrors KIR semantic vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityRef {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
}

/// Named location. Mirrors KIR semantic vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocationRef {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
}

/// Full analysis for one document. Deterministic and serializable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisResult {
    pub version: String,
    pub generator: String,
    pub seed: u64,
    #[serde(default)]
    pub document: DocumentHints,
    #[serde(default)]
    pub chapters: Vec<ChapterAnalysis>,
}

impl AnalysisResult {
    pub fn chapter(&self, id: &str) -> Option<&ChapterAnalysis> {
        self.chapters.iter().find(|c| c.chapter_id == id)
    }

    pub fn to_json(&self) -> Result<String, AnalysisError> {
        serde_json::to_string_pretty(self).map_err(|e| AnalysisError::Serialize(e.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, AnalysisError> {
        let result: Self = serde_json::from_slice(bytes)
            .map_err(|e| AnalysisError::Deserialize(e.to_string()))?;
        if result.version != ANALYSIS_VERSION {
            return Err(AnalysisError::UnsupportedVersion(
                result.version,
                ANALYSIS_VERSION,
            ));
        }
        Ok(result)
    }
}
