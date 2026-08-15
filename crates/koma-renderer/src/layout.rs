//! Text layout: shapes KIR blocks with cosmic-text into positioned glyphs.
//!
//! Layout is independent of rendering: this module produces neutral,
//! positioned glyph lists that any backend can rasterize. Word wrapping,
//! line metrics, and shaping (including per-script shaping) happen here.

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, Wrap};
use koma_core::kir::Block;

use crate::backend::Color;

/// Layout configuration for one page/frame.
#[derive(Debug, Clone, Copy)]
pub struct LayoutConfig {
    pub width: u32,
    pub height: u32,
    pub margin: f32,
    pub font_size: f32,
    pub line_height: f32,
    /// Extra vertical space between blocks (paragraph spacing).
    pub paragraph_spacing: f32,
    pub text_color: Color,
    pub background: Color,
    pub wrap: Wrap,
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
            background: Color::WHITE,
            wrap: Wrap::Word,
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
pub fn layout_blocks(
    font_system: &mut FontSystem,
    blocks: &[Block],
    cfg: &LayoutConfig,
) -> Vec<PlacedLine> {
    let text_width = cfg.width as f32 - 2.0 * cfg.margin;
    let metrics = Metrics {
        font_size: cfg.font_size,
        line_height: cfg.line_height,
    };
    let attrs = Attrs::new();

    let mut lines = Vec::new();
    let mut y = cfg.margin;

    for block in blocks {
        let text = block.plain_text();
        if text.is_empty() {
            continue;
        }
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
                    color: cfg.text_color,
                });
            }
            if !glyphs.is_empty() {
                lines.push(PlacedLine { glyphs });
            }
            y += cfg.line_height;
        }
        y += cfg.paragraph_spacing;
    }

    lines
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
}
