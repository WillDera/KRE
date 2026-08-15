//! Content adapter interfaces (plugin system).
//!
//! Source formats (EPUB, PDF, Markdown, web novels, external apps) exist as
//! adapters that produce KIR. The core engine MUST NOT know about any
//! specific content format; the renderer consumes only KIR.

use crate::kir::Document;

/// A reference to external content to be adapted into KIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSource {
    /// Location of the source (file path, URL, or logical name).
    pub uri: String,
    /// Format hint, e.g. "epub", "pdf", "markdown", "html". Adapters may
    /// inspect content directly and ignore this.
    pub format: Option<String>,
}

impl ContentSource {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            format: None,
        }
    }

    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }
}

/// Error produced while adapting external content into KIR.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("adapter `{0}` does not support source `{1}`")]
    UnsupportedSource(String, String),
    #[error("parse error in `{0}`: {1}")]
    Parse(String, String),
    #[error("missing resource: {0}")]
    MissingResource(String),
    #[error("adapter produced invalid KIR: {0}")]
    InvalidKir(String),
    #[error("source error: {0}")]
    Io(#[from] std::io::Error),
}

/// Converts a source format into KIR.
///
/// Implementations are registered as plugins (plugin type: content adapter)
/// and must be sandboxed with user-approved permissions (AGENTS.md: Plugin
/// Security).
pub trait ContentAdapter {
    /// Stable identifier used for plugin registration, e.g. "koma-epub".
    fn id(&self) -> &'static str;

    /// Whether this adapter can handle the given source.
    fn supports(&self, source: &ContentSource) -> bool;

    /// Convert the source into a KIR [`Document`].
    fn to_kir(&self, source: &ContentSource) -> Result<Document, AdapterError>;
}
