//! XHTML content documents -> KIR blocks.
//!
//! `media_ids` maps a resolved content path (relative to the package base)
//! to a manifest id; images without a manifest match keep their resolved
//! path as `media_id`.

use std::collections::HashMap;

use koma_core::kir::{
    Block, Heading, ImageBlock, ListBlock, Paragraph, QuoteBlock, SpanStyle, TextSpan, block,
    list_block,
};

use crate::EpubError;
use crate::path;
use crate::xml::{is_xhtml, parse};

/// Parse an EPUB content document (XHTML) into KIR blocks.
pub fn parse_content_document(
    bytes: &[u8],
    path: &str,
    media_ids: &HashMap<String, String>,
) -> Result<Vec<Block>, EpubError> {
    let doc = parse(bytes, path)?;
    let root = doc.root_element();
    let mut blocks = Vec::new();
    if let Some(body) = root.descendants().find(|n| is_xhtml(n, "body")) {
        parse_block_children(&body, path, media_ids, &mut blocks);
    }
    Ok(blocks)
}

fn parse_block_children(
    el: &roxmltree::Node,
    doc_path: &str,
    media_ids: &HashMap<String, String>,
    out: &mut Vec<Block>,
) {
    for child in el.children().filter(|n| n.is_element()) {
        if !is_xhtml(&child, child.tag_name().name()) {
            continue;
        }
        let name = child.tag_name().name();
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name[1..].parse().unwrap_or(1);
                let spans = parse_inline(&child);
                out.push(Block {
                    kind: Some(block::Kind::Heading(Heading { level, spans })),
                });
            }
            "p" => out.push(paragraph_block(&child)),
            "img" => out.push(image_block(&child, doc_path, media_ids)),
            "blockquote" => {
                let mut content = Vec::new();
                parse_block_children(&child, doc_path, media_ids, &mut content);
                out.push(Block {
                    kind: Some(block::Kind::Quote(QuoteBlock {
                        content,
                        attribution: None,
                    })),
                });
            }
            "ol" | "ul" => {
                let kind = if name == "ol" {
                    list_block::Kind::Ordered as i32
                } else {
                    list_block::Kind::Unordered as i32
                };
                let mut items = Vec::new();
                for li in child.children().filter(|n| is_xhtml(n, "li")) {
                    let mut inner = Vec::new();
                    parse_block_children(&li, doc_path, media_ids, &mut inner);
                    if inner.is_empty() {
                        inner.push(paragraph_block(&li));
                    }
                    items.extend(inner);
                }
                out.push(Block {
                    kind: Some(block::Kind::List(ListBlock { kind, items })),
                });
            }
            // Structural containers: flatten.
            "div" | "section" | "article" | "aside" | "main" | "figure" | "header" | "footer" => {
                parse_block_children(&child, doc_path, media_ids, out)
            }
            _ => {}
        }
    }
}

fn paragraph_block(el: &roxmltree::Node) -> Block {
    Block {
        kind: Some(block::Kind::Paragraph(Paragraph {
            spans: parse_inline(el),
            annotations: Vec::new(),
            semantic: None,
            style_id: None,
        })),
    }
}

fn image_block(el: &roxmltree::Node, doc_path: &str, media_ids: &HashMap<String, String>) -> Block {
    let src = el.attribute("src").unwrap_or_default();
    let resolved = path::resolve(&path::dir(doc_path), src);
    let media_id = media_ids.get(&resolved).cloned().unwrap_or(resolved);
    Block {
        kind: Some(block::Kind::Image(ImageBlock {
            media_id,
            alt: el.attribute("alt").map(str::to_owned),
            caption: None,
        })),
    }
}

fn parse_inline(el: &roxmltree::Node) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    walk_inline(el, SpanStyle::default(), &mut spans);
    spans
}

fn walk_inline(el: &roxmltree::Node, style: SpanStyle, out: &mut Vec<TextSpan>) {
    for child in el.children() {
        if child.is_text() {
            push_text(out, child.text().unwrap_or_default(), &style);
        } else if child.is_element() {
            let name = child.tag_name().name();
            match name {
                "br" => push_text(out, " ", &style),
                "em" | "i" => {
                    let mut s = style.clone();
                    s.italic = Some(true);
                    walk_inline(&child, s, out);
                }
                "strong" | "b" => {
                    let mut s = style.clone();
                    s.bold = Some(true);
                    walk_inline(&child, s, out);
                }
                "u" => {
                    let mut s = style.clone();
                    s.underline = Some(true);
                    walk_inline(&child, s, out);
                }
                // span, a, sup, sub, code, ... : flatten, preserving style.
                _ => walk_inline(&child, style.clone(), out),
            }
        }
    }
}

fn push_text(out: &mut Vec<TextSpan>, text: &str, style: &SpanStyle) {
    let joined: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        return;
    }
    let style = (style != &SpanStyle::default()).then(|| style.clone());
    if let Some(last) = out.last_mut() {
        if last.style == style {
            let prev = last.text.trim_end();
            let sep = if prev.is_empty() { "" } else { " " };
            last.text = format!("{prev}{sep}{joined}");
            return;
        }
    }
    out.push(TextSpan {
        text: joined,
        language: None,
        style,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Ch</title></head>
<body>
  <h1>Chapter 1</h1>
  <p>A cold <em>coming</em> with <strong>frost</strong>.</p>
  <blockquote><p>Quoted line.</p></blockquote>
  <ul><li>one</li><li>two</li></ul>
  <img src="../Images/pic.png" alt="a picture"/>
</body>
</html>"#;

    #[test]
    fn parses_xhtml_to_blocks() {
        let media = HashMap::from([("OEBPS/Images/pic.png".to_owned(), "pic".to_owned())]);
        let blocks =
            parse_content_document(DOC.as_bytes(), "OEBPS/text/ch1.xhtml", &media).expect("parse");
        assert_eq!(blocks.len(), 5);

        // Heading
        let heading = match &blocks[0].kind {
            Some(block::Kind::Heading(h)) => h,
            _ => panic!("expected heading"),
        };
        assert_eq!(heading.level, 1);
        assert_eq!(span_text(&heading.spans), "Chapter 1");

        // Paragraph with styled spans
        let para = match &blocks[1].kind {
            Some(block::Kind::Paragraph(p)) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(para.spans.len(), 5);
        assert_eq!(para.spans[0].text, "A cold");
        assert_eq!(para.spans[0].style, None);
        assert_eq!(para.spans[1].text, "coming");
        assert_eq!(
            para.spans[1].style.as_ref().map(|s| s.italic),
            Some(Some(true))
        );
        assert_eq!(para.spans[2].text, "with");
        assert_eq!(para.spans[2].style, None);
        assert_eq!(para.spans[3].text, "frost");
        assert_eq!(
            para.spans[3].style.as_ref().map(|s| s.bold),
            Some(Some(true))
        );
        assert_eq!(para.spans[4].text, ".");
        assert_eq!(para.spans[4].style, None);

        // Quote
        assert!(matches!(blocks[2].kind, Some(block::Kind::Quote(_))));

        // List
        let list = match &blocks[3].kind {
            Some(block::Kind::List(l)) => l,
            _ => panic!("expected list"),
        };
        assert_eq!(list.kind, list_block::Kind::Unordered as i32);
        assert_eq!(list.items.len(), 2);

        // Image resolves via manifest
        let img = match &blocks[4].kind {
            Some(block::Kind::Image(i)) => i,
            _ => panic!("expected image"),
        };
        assert_eq!(img.media_id, "pic");
        assert_eq!(img.alt.as_deref(), Some("a picture"));
    }

    fn span_text(spans: &[TextSpan]) -> String {
        spans.iter().map(|s| s.text.clone()).collect()
    }
}
