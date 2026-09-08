//! Markdown event stream -> KIR chapters / blocks.

use koma_core::kir::{
    Block, Chapter, Heading, ImageBlock, ListBlock, Paragraph, QuoteBlock, Section, SpanStyle,
    TextSpan, block, list_block, paragraph,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::front_matter::FrontMatter;

#[derive(Default)]
struct Builder {
    chapters: Vec<Chapter>,
    current_id: Option<String>,
    current_title: Option<String>,
    blocks: Vec<Block>,
    section_n: u32,
    para_spans: Vec<TextSpan>,
    span_style: Option<SpanStyle>,
    heading_level: Option<u32>,
    heading_spans: Vec<TextSpan>,
    list_stack: Vec<ListFrame>,
    quote_depth: u32,
    quote_blocks: Vec<Block>,
    image_dest: Option<String>,
    image_alt: String,
    in_code_block: bool,
    code_buf: String,
}

struct ListFrame {
    ordered: bool,
    items: Vec<Block>,
    item_spans: Vec<TextSpan>,
}

impl Builder {
    fn flush_chapter(&mut self) {
        if self.blocks.is_empty() && self.current_id.is_none() {
            return;
        }
        self.section_n += 1;
        let id = self
            .current_id
            .take()
            .unwrap_or_else(|| format!("ch{}", self.chapters.len() + 1));
        let title = self.current_title.take();
        self.chapters.push(Chapter {
            id,
            title,
            sections: vec![Section {
                id: format!("sec{}", self.section_n),
                blocks: std::mem::take(&mut self.blocks),
            }],
        });
    }

    fn push_paragraph_spans(&mut self, spans: Vec<TextSpan>) {
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        if text.trim().is_empty() {
            return;
        }
        let block = Block {
            kind: Some(block::Kind::Paragraph(Paragraph {
                spans,
                annotations: Vec::new(),
                semantic: None,
                style_id: None,
            })),
        };
        self.push_block(block);
    }

    fn push_block(&mut self, block: Block) {
        if let Some(list) = self.list_stack.last_mut() {
            list.items.push(block);
        } else if self.quote_depth > 0 {
            self.quote_blocks.push(block);
        } else {
            self.blocks.push(block);
        }
    }

    fn finish_para(&mut self) {
        let spans = std::mem::take(&mut self.para_spans);
        if let Some(list) = self.list_stack.last_mut() {
            if !spans.is_empty() {
                list.item_spans.extend(spans);
            }
            return;
        }
        self.push_paragraph_spans(spans);
    }

    fn finish_list_item(&mut self) {
        if let Some(list) = self.list_stack.last_mut() {
            let spans = std::mem::take(&mut list.item_spans);
            if !spans.is_empty() {
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                if !text.trim().is_empty() {
                    list.items.push(Block {
                        kind: Some(block::Kind::Paragraph(Paragraph {
                            spans,
                            annotations: Vec::new(),
                            semantic: None,
                            style_id: None,
                        })),
                    });
                }
            }
        }
    }
}

/// Convert markdown body text into KIR chapters.
///
/// ATX level-1 headings (`#`) start a new chapter. Lower headings become
/// [`Heading`] blocks inside the current chapter. If there is no H1, the
/// whole body is a single chapter `main`.
pub fn markdown_to_chapters(body: &str, fm: &FrontMatter) -> Vec<Chapter> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut b = Builder::default();
    for event in Parser::new_ext(body, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                b.finish_para();
                b.heading_level = Some(level as u32);
                b.heading_spans.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                let level = b.heading_level.take().unwrap_or(1);
                let title: String = b.heading_spans.iter().map(|s| s.text.as_str()).collect();
                if level == 1 {
                    b.flush_chapter();
                    b.current_id = Some(slug(&title, b.chapters.len() + 1));
                    b.current_title = Some(title);
                    b.blocks.push(Block {
                        kind: Some(block::Kind::Heading(Heading {
                            level: 1,
                            spans: std::mem::take(&mut b.heading_spans),
                        })),
                    });
                } else {
                    b.blocks.push(Block {
                        kind: Some(block::Kind::Heading(Heading {
                            level,
                            spans: std::mem::take(&mut b.heading_spans),
                        })),
                    });
                }
            }
            Event::Start(Tag::Paragraph) => {
                b.para_spans.clear();
                b.span_style = None;
            }
            Event::End(TagEnd::Paragraph) => b.finish_para(),
            Event::Start(Tag::Emphasis) => {
                b.span_style = Some(SpanStyle {
                    bold: None,
                    italic: Some(true),
                    underline: None,
                    color: None,
                });
            }
            Event::Start(Tag::Strong) => {
                b.span_style = Some(SpanStyle {
                    bold: Some(true),
                    italic: None,
                    underline: None,
                    color: None,
                });
            }
            Event::End(TagEnd::Emphasis) | Event::End(TagEnd::Strong) => {
                b.span_style = None;
            }
            Event::Text(t) => {
                if b.in_code_block {
                    b.code_buf.push_str(&t);
                    continue;
                }
                let span = TextSpan {
                    text: t.to_string(),
                    language: None,
                    style: b.span_style.clone(),
                };
                if b.heading_level.is_some() {
                    b.heading_spans.push(span);
                } else if b.image_dest.is_some() {
                    b.image_alt.push_str(&t);
                } else if let Some(list) = b.list_stack.last_mut() {
                    list.item_spans.push(span);
                } else {
                    b.para_spans.push(span);
                }
            }
            Event::Code(t) => {
                let span = TextSpan {
                    text: t.to_string(),
                    language: None,
                    style: None,
                };
                if b.heading_level.is_some() {
                    b.heading_spans.push(span);
                } else {
                    b.para_spans.push(span);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                let span = TextSpan {
                    text: " ".into(),
                    language: None,
                    style: None,
                };
                if b.heading_level.is_some() {
                    b.heading_spans.push(span);
                } else if let Some(list) = b.list_stack.last_mut() {
                    list.item_spans.push(span);
                } else {
                    b.para_spans.push(span);
                }
            }
            Event::Start(Tag::List(start)) => {
                b.list_stack.push(ListFrame {
                    ordered: start.is_some(),
                    items: Vec::new(),
                    item_spans: Vec::new(),
                });
            }
            Event::End(TagEnd::List(_)) => {
                if let Some(frame) = b.list_stack.pop() {
                    b.push_block(Block {
                        kind: Some(block::Kind::List(ListBlock {
                            kind: if frame.ordered {
                                list_block::Kind::Ordered as i32
                            } else {
                                list_block::Kind::Unordered as i32
                            },
                            items: frame.items,
                        })),
                    });
                }
            }
            Event::Start(Tag::Item) => {
                if let Some(list) = b.list_stack.last_mut() {
                    list.item_spans.clear();
                }
            }
            Event::End(TagEnd::Item) => b.finish_list_item(),
            Event::Start(Tag::BlockQuote(_)) => {
                b.quote_depth += 1;
                if b.quote_depth == 1 {
                    b.quote_blocks.clear();
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if b.quote_depth > 0 {
                    b.quote_depth -= 1;
                }
                if b.quote_depth == 0 {
                    let content = std::mem::take(&mut b.quote_blocks);
                    b.blocks.push(Block {
                        kind: Some(block::Kind::Quote(QuoteBlock {
                            content,
                            attribution: None,
                        })),
                    });
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                b.image_dest = Some(dest_url.to_string());
                b.image_alt.clear();
            }
            Event::End(TagEnd::Image) => {
                if let Some(dest) = b.image_dest.take() {
                    let alt = std::mem::take(&mut b.image_alt);
                    b.push_block(Block {
                        kind: Some(block::Kind::Image(ImageBlock {
                            media_id: dest_to_media_id(&dest),
                            alt: if alt.is_empty() { None } else { Some(alt) },
                            caption: None,
                        })),
                    });
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                b.in_code_block = true;
                b.code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                b.in_code_block = false;
                let text = std::mem::take(&mut b.code_buf);
                if !text.trim().is_empty() {
                    b.push_block(Block {
                        kind: Some(block::Kind::Paragraph(paragraph(text.trim_end()))),
                    });
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let stripped = strip_simple_html(&html);
                if !stripped.is_empty() {
                    b.para_spans.push(TextSpan {
                        text: stripped,
                        language: None,
                        style: None,
                    });
                }
            }
            _ => {}
        }
    }

    b.flush_chapter();
    if b.chapters.is_empty() {
        b.chapters.push(Chapter {
            id: "main".into(),
            title: fm.title.clone(),
            sections: vec![Section {
                id: "sec1".into(),
                blocks: Vec::new(),
            }],
        });
    }
    b.chapters
}

fn slug(title: &str, fallback_n: usize) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() {
        format!("ch{fallback_n}")
    } else {
        s
    }
}

fn dest_to_media_id(dest: &str) -> String {
    dest.rsplit(['/', '\\'])
        .next()
        .unwrap_or(dest)
        .trim()
        .to_owned()
}

fn strip_simple_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use koma_core::kir::block;

    #[test]
    fn splits_on_h1_and_keeps_inline_styles() {
        let body = "# Arrival\n\nA cold *coming*.\n\n## Deep\n\nSnow fell.\n\n# Departure\n\nGone.\n";
        let chapters = markdown_to_chapters(body, &FrontMatter::default());
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title.as_deref(), Some("Arrival"));
        assert_eq!(chapters[1].title.as_deref(), Some("Departure"));
        let blocks = &chapters[0].sections[0].blocks;
        assert!(matches!(blocks[0].kind, Some(block::Kind::Heading(_))));
        let para = match &blocks[1].kind {
            Some(block::Kind::Paragraph(p)) => p,
            _ => panic!("expected paragraph"),
        };
        assert!(para.spans.iter().any(|s| s.text == "coming" && s.style.as_ref().and_then(|st| st.italic) == Some(true)));
    }

    #[test]
    fn single_chapter_without_h1() {
        let body = "Just a paragraph.\n\nAnd another.\n";
        let chapters = markdown_to_chapters(body, &FrontMatter::default());
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].id, "ch1");
        assert_eq!(chapters[0].sections[0].blocks.len(), 2);
    }
}
