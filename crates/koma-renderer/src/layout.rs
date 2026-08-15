//! Text layout: shapes KIR blocks with cosmic-text into positioned glyphs.
//!
//! Layout is independent of rendering: this module produces neutral,
//! positioned glyph lists that any backend can rasterize. Word wrapping,
//! line metrics, and shaping (including per-script shaping) happen here.

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};
use koma_core::kir::Block;

use crate::backend::Color;

/// Layout configuration for one page/frame.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub width: u32,
    pub height: u32,
    pub margin: f32,
    pub font_size: f32,
    pub line_height: f32,
    /// Extra vertical space between blocks (paragraph spacing).
    pub paragraph_spacing: f32,
    pub text_color: Color,
    pub heading_color: Color,
    pub quote_color: Color,
    pub background: Color,
    pub wrap: Wrap,
    /// Named font family (theme typography); fallback to default when absent.
    pub font_family: Option<String>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 1000,
            margin: 48.0,
            font_size: 20.0,
            line_height: 28.0,
            paragraph_spacing: 12.0,
            text_color: Color::BLACK,
            heading_color: Color::rgb(0x1a, 0x1a, 0x2e),
            quote_color: Color::rgb(0x44, 0x44, 0x44),
            background: Color::WHITE,
            wrap: Wrap::Word,
            font_family: None,
        }
    }
}

/// A single placed glyph: absolute frame position + shaping metadata needed
/// by the rasterizer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub font_id: fontdb::ID,
    pub glyph_id: u16,
    /// Device-pixel size used during shaping.
    pub font_size: f32,
    /// Glyph bounding-box top-left, absolute in frame pixels.
    pub x: i32,
    pub y: i32,
    /// Sub-pixel offsets for precise rasterization.
    pub offset_x: f32,
    pub offset_y: f32,
    pub color: Color,
}

/// One laid-out line of placed glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLine {
    pub glyphs: Vec<PlacedGlyph>,
}

/// Lay out a batch of KIR blocks into positioned glyph lines, wrapped to the
/// configured width. Non-text blocks (images) are skipped here.
///
/// Presentation mapping: headings scale up and use `heading_color`, quotes
/// use `quote_color`, everything else uses `text_color`. `font_family`
/// selects the named family (default when absent or uninstalled).
pub fn layout_blocks(
    font_system: &mut FontSystem,
    blocks: &[Block],
    cfg: &LayoutConfig,
) -> Vec<PlacedLine> {
    let text_width = cfg.width as f32 - 2.0 * cfg.margin;
    let attrs = match &cfg.font_family {
        Some(family) => Attrs::new().family(Family::Name(family)),
        None => Attrs::new(),
    };

    let mut lines = Vec::new();
    let mut y = cfg.margin;

    for block in blocks {
        let text = block.plain_text();
        if text.is_empty() {
            continue;
        }
        let color = block_color(block, cfg);
        let (metrics, paragraph_spacing) = block_metrics(block, cfg);
        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_size(font_system, Some(text_width), None);
        buffer.set_wrap(font_system, cfg.wrap);
        buffer.set_text(font_system, &text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);

        for run in buffer.layout_runs() {
            let mut glyphs = Vec::with_capacity(run.glyphs.len());
            for glyph in run.glyphs {
                let physical = glyph.physical((cfg.margin, y), 1.0);
                glyphs.push(PlacedGlyph {
                    font_id: glyph.font_id,
                    glyph_id: glyph.glyph_id,
                    font_size: glyph.font_size,
                    x: physical.x,
                    y: physical.y,
                    offset_x: physical.cache_key.x_bin.as_float(),
                    offset_y: physical.cache_key.y_bin.as_float(),
                    color,
                });
            }
            if !glyphs.is_empty() {
                lines.push(PlacedLine { glyphs });
            }
            y += metrics.line_height;
        }
        y += paragraph_spacing;
    }

    lines
}

/// Per-block presentation color (presentation truth only; never content).
fn block_color(block: &Block, cfg: &LayoutConfig) -> Color {
    match &block.kind {
        Some(koma_core::kir::block::Kind::Heading(_)) => cfg.heading_color,
        Some(koma_core::kir::block::Kind::Quote(_)) => cfg.quote_color,
        _ => cfg.text_color,
    }
}

/// Per-block metrics: headings scale up relative to body text.
fn block_metrics(block: &Block, cfg: &LayoutConfig) -> (Metrics, f32) {
    match &block.kind {
        Some(koma_core::kir::block::Kind::Heading(h)) => {
            let scale = if h.level <= 1 { 1.5 } else { 1.25 };
            let size = cfg.font_size * scale;
            (
                Metrics {
                    font_size: size,
                    line_height: cfg.line_height * scale,
                },
                cfg.paragraph_spacing,
            )
        }
        _ => (
            Metrics {
                font_size: cfg.font_size,
                line_height: cfg.line_height,
            },
            cfg.paragraph_spacing,
        ),
    }
}

#[cfg(test)]
mod tests {
    use cosmic_text::FontSystem;

    use koma_core::kir::Block;

    use super::*;

    fn blocks_with(texts: &[&str]) -> Vec<Block> {
        texts
            .iter()
            .map(|t| Block {
                kind: Some(koma_core::kir::block::Kind::Paragraph(
                    koma_core::kir::paragraph(*t),
                )),
            })
            .collect()
    }

    #[test]
    fn wraps_long_paragraph_into_multiple_lines() {
        let mut fs = FontSystem::new();
        let cfg = LayoutConfig {
            width: 300,
            height: 600,
            margin: 10.0,
            font_size: 16.0,
            line_height: 22.0,
            paragraph_spacing: 4.0,
            ..Default::default()
        };
        let long = "one two three four five six seven eight nine ten";
        let lines = layout_blocks(&mut fs, &blocks_with(&[long]), &cfg);
        assert!(
            lines.len() >= 2,
            "expected wrapping, got {} lines",
            lines.len()
        );
        // Every glyph sits inside the frame width.
        for line in &lines {
            for g in &line.glyphs {
                assert!(g.x >= 0 && g.x < cfg.width as i32);
            }
        }
    }

    #[test]
    fn short_text_stays_on_one_line_and_layout_is_monotonic() {
        let mut fs = FontSystem::new();
        let cfg = LayoutConfig {
            width: 800,
            height: 400,
            margin: 10.0,
            font_size: 16.0,
            line_height: 22.0,
            paragraph_spacing: 4.0,
            ..Default::default()
        };
        let lines = layout_blocks(&mut fs, &blocks_with(&["A cold coming."]), &cfg);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].glyphs.is_empty());

        // Two blocks stack vertically: first line y < second line y.
        let two = layout_blocks(&mut fs, &blocks_with(&["alpha", "bravo"]), &cfg);
        assert_eq!(two.len(), 2);
        let first_y = two[0].glyphs.iter().map(|g| g.y).min().unwrap();
        let second_y = two[1].glyphs.iter().map(|g| g.y).min().unwrap();
        assert!(second_y > first_y);
    }

    #[test]
    fn cjk_text_shapes_to_nonempty_glyphs() {
        let mut fs = FontSystem::new();
        let cfg = LayoutConfig {
            width: 800,
            height: 200,
            ..Default::default()
        };
        let lines = layout_blocks(&mut fs, &blocks_with(&["冰封世界的漫长冬日"]), &cfg);
        assert_eq!(lines.len(), 1, "CJK line should not wrap in a wide frame");
        assert!(!lines[0].glyphs.is_empty(), "CJK should shape to glyphs");
    }

    #[test]
    fn headings_scale_up_and_use_heading_color() {
        let mut fs = FontSystem::new();
        let cfg = LayoutConfig {
            width: 800,
            height: 400,
            font_size: 16.0,
            line_height: 22.0,
            ..Default::default()
        };
        let blocks = vec![
            Block {
                kind: Some(koma_core::kir::block::Kind::Heading(
                    koma_core::kir::Heading {
                        level: 1,
                        spans: vec![koma_core::kir::TextSpan {
                            text: "Chapter One".to_owned(),
                            language: None,
                            style: None,
                        }],
                    },
                )),
            },
            Block {
                kind: Some(koma_core::kir::block::Kind::Paragraph(
                    koma_core::kir::paragraph("body text"),
                )),
            },
        ];
        let lines = layout_blocks(&mut fs, &blocks, &cfg);
        assert_eq!(lines.len(), 2);
        let heading_size = lines[0].glyphs.iter().map(|g| g.font_size).next().unwrap();
        let body_size = lines[1].glyphs.iter().map(|g| g.font_size).next().unwrap();
        assert!(heading_size > body_size, "heading should scale up");
        assert_eq!(lines[0].glyphs[0].color, cfg.heading_color);
        assert_eq!(lines[1].glyphs[0].color, cfg.text_color);
    }

    #[test]
    fn named_font_family_degrades_gracefully() {
        let mut fs = FontSystem::new();
        // Even an unknown family must not break layout: fallback to default.
        let cfg = LayoutConfig {
            width: 600,
            height: 100,
            font_family: Some("Definitely-Not-A-Real-Font".to_owned()),
            ..Default::default()
        };
        let lines = layout_blocks(&mut fs, &blocks_with(&["readable"]), &cfg);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].glyphs.is_empty());
    }
}
