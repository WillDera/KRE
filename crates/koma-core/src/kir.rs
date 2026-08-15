//! Koma Intermediate Representation (KIR).
//!
//! Types are generated from `proto/kir.proto` by `prost-build`. This module
//! exposes them under the `kir` path together with small KIR-level helpers.
//!
//! A `Chapter` is the independent serialization boundary: the runtime streams
//! chapters lazily and must never require a whole `Document` in memory.

include!(concat!(env!("OUT_DIR"), "/koma.kir.rs"));

use crate::{EncodeError, KirError};

/// KIR schema version carried in `Document::version`.
pub const KIR_VERSION: &str = "0.1.0";

impl Document {
    /// Create a minimal, valid document with the current schema version.
    pub fn new() -> Self {
        Self {
            version: KIR_VERSION.to_owned(),
            metadata: None,
            chapters: Vec::new(),
        }
    }
}

impl Chapter {
    /// A chapter is the lazy-loading boundary: it can be encoded and decoded
    /// independently of the rest of the document.
    pub fn encode_chapter(&self) -> Result<Vec<u8>, EncodeError> {
        let mut buf = Vec::new();
        prost::Message::encode(self, &mut buf)?;
        Ok(buf)
    }

    pub fn decode_chapter(bytes: &[u8]) -> Result<Self, KirError> {
        Ok(prost::Message::decode(bytes)?)
    }
}

/// Build a paragraph from a plain text string (single span, no metadata).
/// External applications may send exactly this shape and Koma must render it.
pub fn paragraph(text: impl Into<String>) -> Paragraph {
    Paragraph {
        spans: vec![TextSpan {
            text: text.into(),
            language: None,
            style: None,
        }],
        annotations: Vec::new(),
        semantic: None,
        style_id: None,
    }
}

impl Paragraph {
    /// Attach semantic metadata (entities, locations, mood, environment) to a
    /// paragraph. Semantic truth never alters content truth.
    pub fn with_semantic(mut self, semantic: SemanticMetadata) -> Self {
        self.semantic = Some(semantic);
        self
    }
}

#[cfg(test)]
mod tests {
    use prost::Message as _;

    use super::*;

    fn sample_document() -> Document {
        let mut doc = Document::new();
        doc.metadata = Some(DocumentMetadata {
            title: Some("The Long Dark".to_owned()),
            author: Some("Big Dog".to_owned()),
            language: Some("en".to_owned()),
            identifiers: vec![Identifier {
                scheme: "uuid".to_owned(),
                value: "11111111-2222-3333-4444-555555555555".to_owned(),
            }],
            license: None,
            generator: "koma-compiler 0.1.0".to_owned(),
            seed: 42,
        });

        let para = paragraph("A cold coming.").with_semantic(SemanticMetadata {
            entities: vec![EntityRef {
                r#type: "character".to_owned(),
                name: "Inquisitor".to_owned(),
            }],
            locations: vec![LocationRef {
                r#type: "place".to_owned(),
                name: "Chamber".to_owned(),
            }],
            themes: Vec::new(),
            mood: Some("ominous".to_owned()),
            environment: Some("frozen".to_owned()),
        });

        let mut chapter = Chapter {
            id: "ch-1".to_owned(),
            title: Some("Arrival".to_owned()),
            sections: Vec::new(),
        };
        chapter.sections.push(Section {
            id: "sec-1".to_owned(),
            blocks: vec![Block {
                kind: Some(block::Kind::Paragraph(para)),
            }],
        });
        doc.chapters.push(chapter);
        doc
    }

    #[test]
    fn document_round_trips() {
        let doc = sample_document();
        let mut buf = Vec::new();
        prost::Message::encode(&doc, &mut buf).expect("encode");
        let decoded = Document::decode(buf.as_slice()).expect("decode");

        assert_eq!(doc, decoded);
        assert_eq!(decoded.version, KIR_VERSION);
        assert_eq!(decoded.chapters.len(), 1);
        assert_eq!(decoded.chapters[0].id, "ch-1");
    }

    #[test]
    fn chapter_is_independent_serialization_boundary() {
        let doc = sample_document();
        let chapter = &doc.chapters[0];

        let bytes = chapter.encode_chapter().expect("encode chapter");
        let decoded = Chapter::decode_chapter(&bytes).expect("decode chapter");

        assert_eq!(decoded, *chapter);
    }

    #[test]
    fn external_application_paragraph_renders() {
        // A third-party app sends only a paragraph with metadata; Koma renders
        // it as-is (AGENTS.md example).
        let p = paragraph("A cold coming.").with_semantic(SemanticMetadata {
            entities: Vec::new(),
            locations: Vec::new(),
            themes: Vec::new(),
            mood: Some("ominous".to_owned()),
            environment: Some("frozen".to_owned()),
        });

        let mut buf = Vec::new();
        prost::Message::encode(&p, &mut buf).expect("encode paragraph");
        let decoded = Paragraph::decode(buf.as_slice()).expect("decode paragraph");

        assert_eq!(p, decoded);
        assert_eq!(
            decoded.semantic.as_ref().map(|s| s.mood.as_deref()),
            Some(Some("ominous"))
        );
        assert_eq!(
            decoded.semantic.as_ref().map(|s| s.environment.as_deref()),
            Some(Some("frozen"))
        );
    }

    #[test]
    fn block_oneof_variants_round_trip() {
        let blocks = vec![
            Block {
                kind: Some(block::Kind::Paragraph(paragraph("text"))),
            },
            Block {
                kind: Some(block::Kind::Heading(Heading {
                    level: 2,
                    spans: vec![TextSpan {
                        text: "Title".to_owned(),
                        language: None,
                        style: None,
                    }],
                })),
            },
            Block {
                kind: Some(block::Kind::Image(ImageBlock {
                    media_id: "img-1".to_owned(),
                    alt: Some("alt".to_owned()),
                    caption: None,
                })),
            },
            Block {
                kind: Some(block::Kind::Quote(QuoteBlock {
                    content: vec![Block {
                        kind: Some(block::Kind::Paragraph(paragraph("quoted"))),
                    }],
                    attribution: Some("Someone".to_owned()),
                })),
            },
            Block {
                kind: Some(block::Kind::List(ListBlock {
                    kind: list_block::Kind::Ordered as i32,
                    items: vec![Block {
                        kind: Some(block::Kind::Paragraph(paragraph("item"))),
                    }],
                })),
            },
        ];

        for block in blocks {
            let mut buf = Vec::new();
            prost::Message::encode(&block, &mut buf).expect("encode block");
            let decoded = Block::decode(buf.as_slice()).expect("decode block");
            assert_eq!(block, decoded);
        }
    }

    #[test]
    fn empty_document_has_current_version() {
        assert_eq!(Document::new().version, KIR_VERSION);
    }
}
