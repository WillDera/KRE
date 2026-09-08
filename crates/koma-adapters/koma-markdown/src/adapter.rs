//! [`MarkdownAdapter`]: Markdown -> KIR content adapter.

use std::fs;
use std::path::Path;

use koma_core::adapters::{AdapterError, ContentAdapter, ContentSource};
use koma_core::kir::{Document, DocumentMetadata, KIR_VERSION};

use crate::convert::markdown_to_chapters;
use crate::front_matter::split_front_matter;

/// Markdown content adapter. Registered as a `ContentAdapter` plugin.
pub struct MarkdownAdapter;

impl MarkdownAdapter {
    /// Adapt markdown bytes/string directly (useful for tests and HTTP bodies).
    pub fn to_kir_str(&self, input: &str, source_name: &str) -> Result<Document, AdapterError> {
        let (fm, body) = split_front_matter(input);
        let chapters = markdown_to_chapters(body, &fm);
        let seed = fnv1a(input.as_bytes());
        let title = fm.title.or_else(|| {
            chapters
                .first()
                .and_then(|c| c.title.clone())
                .or_else(|| Some(Path::new(source_name).file_stem()?.to_string_lossy().into_owned()))
        });
        Ok(Document {
            version: KIR_VERSION.to_owned(),
            metadata: Some(DocumentMetadata {
                title,
                author: fm.author,
                language: fm.language,
                identifiers: Vec::new(),
                license: None,
                generator: format!("koma-markdown {}", env!("CARGO_PKG_VERSION")),
                seed,
            }),
            chapters,
        })
    }
}

impl ContentAdapter for MarkdownAdapter {
    fn id(&self) -> &'static str {
        "koma-markdown"
    }

    fn supports(&self, source: &ContentSource) -> bool {
        let fmt = source.format.as_deref().map(|s| s.to_ascii_lowercase());
        matches!(fmt.as_deref(), Some("markdown") | Some("md"))
            || source.uri.to_lowercase().ends_with(".md")
            || source.uri.to_lowercase().ends_with(".markdown")
    }

    fn to_kir(&self, source: &ContentSource) -> Result<Document, AdapterError> {
        let text = fs::read_to_string(&source.uri).map_err(AdapterError::Io)?;
        self.to_kir_str(&text, &source.uri)
    }
}

/// Error type for Markdown adaptation.
#[derive(Debug, thiserror::Error)]
pub enum MarkdownError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<MarkdownError> for AdapterError {
    fn from(e: MarkdownError) -> Self {
        match e {
            MarkdownError::Parse(m) => AdapterError::Parse("markdown".into(), m),
            MarkdownError::Io(io) => AdapterError::Io(io),
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const PRIME: u64 = 1_099_511_628_211;
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ *b as u64).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn supports_md_sources() {
        let a = MarkdownAdapter;
        assert!(a.supports(&ContentSource::new("book.md")));
        assert!(a.supports(&ContentSource::new("book.markdown")));
        assert!(a.supports(&ContentSource::new("x").with_format("markdown")));
        assert!(!a.supports(&ContentSource::new("book.epub")));
    }

    #[test]
    fn adapts_file_with_front_matter() {
        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            "---\ntitle: The Long Dark\nauthor: Big Dog\nlanguage: en\n---\n\n# Arrival\n\nA cold coming.\n"
        )
        .unwrap();
        f.flush().unwrap();
        let doc = MarkdownAdapter
            .to_kir(&ContentSource::new(f.path().to_string_lossy().into_owned()))
            .unwrap();
        let meta = doc.metadata.as_ref().unwrap();
        assert_eq!(meta.title.as_deref(), Some("The Long Dark"));
        assert_eq!(meta.author.as_deref(), Some("Big Dog"));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert!(meta.generator.starts_with("koma-markdown "));
        assert_eq!(doc.chapters.len(), 1);
        assert_eq!(doc.chapters[0].title.as_deref(), Some("Arrival"));
    }
}
