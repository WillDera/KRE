//! `.koma` package manifest (JSON), the package-level metadata.

use serde::{Deserialize, Serialize};

/// Format identifier carried by every `.koma` package.
pub const KOMA_FORMAT: &str = "koma";
/// `.koma` package format version (breaking changes require migration).
pub const KOMA_PACKAGE_VERSION: &str = "0.1.0";
/// Zip entry containing the package manifest.
pub const MANIFEST_PATH: &str = "koma.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageManifest {
    pub format: String,
    pub version: String,
    pub generator: GeneratorInfo,
    /// Deterministic seed derived from content (reproducibility).
    pub seed: u64,
    pub document: DocumentInfo,
    pub chapters: Vec<ChapterEntry>,
    pub assets: Vec<AssetEntry>,
    /// Theme information path (theme engine, Phase 5).
    pub theme: Option<String>,
    /// Scene data path (scene graph, Phase 6).
    pub scene: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratorInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub identifiers: Vec<IdentifierInfo>,
    pub license: Option<LicenseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentifierInfo {
    pub scheme: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenseInfo {
    pub name: Option<String>,
    pub creator: Option<String>,
    pub license: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
}

/// One chapter in the package; the runtime loads these lazily.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterEntry {
    pub id: String,
    pub title: Option<String>,
    /// Zip path of the protobuf-encoded chapter.
    pub path: String,
    /// Encoded chapter size in bytes.
    pub bytes: usize,
}

/// A media reference embedded in (or expected by) the package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetEntry {
    pub id: String,
    /// Zip path, `Some` when bytes are embedded in the package.
    pub path: Option<String>,
    pub mime: Option<String>,
    pub bytes: usize,
}
