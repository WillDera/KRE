//! Selection, highlight rects, and user marks on the layout layer.
//!
//! Ranges address `Block::plain_text()` byte offsets — never raster pixels.
//! User marks are presentation/user state and must never be written into KIR
//! or `.koma` packages (AGENTS.md User State System).

use koma_core::kir::Block;

use crate::backend::Color;
use crate::layout::{PaginatedLayout, TextHit};

/// A caret / range endpoint: block index + byte offset into that block's
/// plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextAnchor {
    pub block: usize,
    pub offset: u32,
}

impl TextAnchor {
    pub fn new(block: usize, offset: u32) -> Self {
        Self { block, offset }
    }

    pub fn from_hit(hit: TextHit) -> Self {
        Self {
            block: hit.block,
            offset: hit.start,
        }
    }
}

/// Half-open text range `[start, end)` in content-layer coordinates.
/// Always normalized so `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRange {
    pub start: TextAnchor,
    pub end: TextAnchor,
}

impl TextRange {
    /// Build a normalized range from two anchors.
    pub fn new(a: TextAnchor, b: TextAnchor) -> Self {
        if a <= b {
            Self { start: a, end: b }
        } else {
            Self { start: b, end: a }
        }
    }

    /// Build a normalized range covering both hit-test results (inclusive of
    /// each hit's glyph byte span).
    pub fn from_hits(a: TextHit, b: TextHit) -> Self {
        let points = [
            TextAnchor::new(a.block, a.start),
            TextAnchor::new(a.block, a.end),
            TextAnchor::new(b.block, b.start),
            TextAnchor::new(b.block, b.end),
        ];
        let start = *points.iter().min().expect("non-empty");
        let end = *points.iter().max().expect("non-empty");
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Whether this range covers any bytes of `block` at `[offset_start, offset_end)`.
    pub fn overlaps_block_bytes(&self, block: usize, offset_start: u32, offset_end: u32) -> bool {
        if offset_start >= offset_end {
            return false;
        }
        let range_start = if block == self.start.block {
            self.start.offset
        } else if block > self.start.block {
            0
        } else {
            return false;
        };
        let range_end = if block == self.end.block {
            self.end.offset
        } else if block < self.end.block {
            u32::MAX
        } else {
            return false;
        };
        offset_start < range_end && offset_end > range_start
    }
}

/// Axis-aligned highlight underlay in page/frame device pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Kind of user mark. Mirrors KIR annotation vocabulary but lives in user
/// state, not in content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkKind {
    Highlight,
    Note,
    Bookmark,
}

/// A user annotation stored separately from `.koma` packages.
#[derive(Debug, Clone, PartialEq)]
pub struct UserMark {
    pub id: String,
    pub kind: MarkKind,
    pub range: TextRange,
    pub note: Option<String>,
    /// Highlight underlay color; default applied when `None`.
    pub color: Option<Color>,
}

impl UserMark {
    pub fn highlight(id: impl Into<String>, range: TextRange) -> Self {
        Self {
            id: id.into(),
            kind: MarkKind::Highlight,
            range,
            note: None,
            color: None,
        }
    }

    pub fn resolve_color(&self) -> Color {
        self.color.unwrap_or(Color {
            r: 255,
            g: 230,
            b: 80,
            a: 96,
        })
    }
}

impl PaginatedLayout {
    /// Drag-select between two points on `page`. Returns `None` if either
    /// point misses text.
    pub fn select(&self, page: usize, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<TextRange> {
        let a = self.hit_test(page, x0, y0)?;
        let b = self.hit_test(page, x1, y1)?;
        Some(TextRange::from_hits(a, b))
    }

    /// Glyph-backed highlight rectangles for `range` on `page`.
    ///
    /// Consecutive overlapping glyphs on the same line are merged into one
    /// rect. Empty ranges yield no rects.
    pub fn rects_for_range(&self, page: usize, range: &TextRange) -> Vec<HighlightRect> {
        if range.is_empty() {
            return Vec::new();
        }
        let Some(page) = self.pages.get(page) else {
            return Vec::new();
        };

        let mut rects = Vec::new();
        for (li, line) in page.lines.iter().enumerate() {
            let block = page.line_blocks[li];
            let mut run: Option<(f32, f32, f32, f32)> = None; // x0, x1, y, h

            let flush = |run: &mut Option<(f32, f32, f32, f32)>, rects: &mut Vec<HighlightRect>| {
                if let Some((x0, x1, y, h)) = run.take() {
                    rects.push(HighlightRect {
                        x: x0,
                        y,
                        width: (x1 - x0).max(1.0),
                        height: h,
                    });
                }
            };

            for (gi, g) in line.glyphs.iter().enumerate() {
                if !range.overlaps_block_bytes(block, g.byte_range.0, g.byte_range.1) {
                    flush(&mut run, &mut rects);
                    continue;
                }
                let gx0 = g.x as f32;
                let gx1 = line
                    .glyphs
                    .get(gi + 1)
                    .map(|n| n.x as f32)
                    .unwrap_or(gx0 + g.font_size.max(1.0));
                let gy = g.y as f32;
                let gh = line.line_height;
                match &mut run {
                    Some((x0, x1, y, h)) if (*y - gy).abs() < 0.5 => {
                        *x0 = x0.min(gx0);
                        *x1 = x1.max(gx1);
                        *h = h.max(gh);
                    }
                    _ => {
                        flush(&mut run, &mut rects);
                        run = Some((gx0, gx1, gy, gh));
                    }
                }
            }
            flush(&mut run, &mut rects);
        }
        rects
    }
}

/// Slice `blocks` plain text covered by `range` (content layer, for clipboard
/// and accessibility).
pub fn extract_plain_text(blocks: &[Block], range: &TextRange) -> String {
    if range.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for bi in range.start.block..=range.end.block.min(blocks.len().saturating_sub(1)) {
        let Some(block) = blocks.get(bi) else {
            break;
        };
        let text = block.plain_text();
        let bytes = text.as_bytes();
        let start = if bi == range.start.block {
            (range.start.offset as usize).min(bytes.len())
        } else {
            0
        };
        let end = if bi == range.end.block {
            (range.end.offset as usize).min(bytes.len())
        } else {
            bytes.len()
        };
        if start >= end {
            continue;
        }
        // Stay on UTF-8 boundaries.
        let start = floor_char_boundary(&text, start);
        let end = ceil_char_boundary(&text, end);
        if start < end {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text[start..end]);
        }
    }
    out
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Paint highlight underlays into `frame` (before glyphs).
pub fn paint_rects(frame: &mut crate::backend::Frame, rects: &[HighlightRect], color: Color) {
    for r in rects {
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.width).ceil() as i32;
        let y1 = (r.y + r.height).ceil() as i32;
        frame.fill_rect(x0, y0, x1 - x0, y1 - y0, color);
    }
}

#[cfg(test)]
mod tests {
    use cosmic_text::FontSystem;

    use koma_core::kir::Block;

    use super::*;
    use crate::layout::{LayoutConfig, paginate_blocks};

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

    fn cfg() -> LayoutConfig {
        LayoutConfig {
            width: 800,
            height: 400,
            margin: 10.0,
            font_size: 16.0,
            line_height: 22.0,
            paragraph_spacing: 4.0,
            ..Default::default()
        }
    }

    #[test]
    fn range_from_hits_normalizes_order() {
        let a = TextHit {
            block: 0,
            start: 5,
            end: 6,
        };
        let b = TextHit {
            block: 0,
            start: 1,
            end: 2,
        };
        let r = TextRange::from_hits(a, b);
        assert_eq!(r.start.offset, 1);
        assert_eq!(r.end.offset, 6);
    }

    #[test]
    fn select_and_extract_plain_text() {
        let mut fs = FontSystem::new();
        let text = "The old inquisitor";
        let blocks = blocks_with(&[text]);
        let paginated = paginate_blocks(&mut fs, &blocks, &cfg());
        let page = &paginated.pages[0];
        let line = &page.lines[0];
        let g0 = line.glyphs[0];
        let g3 = line.glyphs[3];
        let range = paginated
            .select(
                0,
                g0.x as f32 + 1.0,
                g0.y as f32 + 2.0,
                g3.x as f32 + 1.0,
                g3.y as f32 + 2.0,
            )
            .expect("selection");
        assert!(!range.is_empty());
        let sliced = extract_plain_text(&blocks, &range);
        assert!(
            text.starts_with(&sliced) || sliced.chars().all(|c| text.contains(c)),
            "sliced={sliced:?}"
        );
        assert!(!sliced.is_empty());
    }

    #[test]
    fn rects_for_range_cover_selected_glyphs() {
        let mut fs = FontSystem::new();
        let text = "ABCDEFGH";
        let blocks = blocks_with(&[text]);
        let paginated = paginate_blocks(&mut fs, &blocks, &cfg());
        let page = &paginated.pages[0];
        let line = &page.lines[0];
        let g0 = line.glyphs[0];
        let g2 = line.glyphs[2];
        let range = TextRange::from_hits(
            TextHit {
                block: 0,
                start: g0.byte_range.0,
                end: g0.byte_range.1,
            },
            TextHit {
                block: 0,
                start: g2.byte_range.0,
                end: g2.byte_range.1,
            },
        );
        let rects = paginated.rects_for_range(0, &range);
        assert!(!rects.is_empty(), "expected highlight rects");
        assert!(rects[0].width > 0.0 && rects[0].height > 0.0);
        // First rect should sit near the first glyph.
        assert!((rects[0].x - g0.x as f32).abs() < 2.0);
    }

    #[test]
    fn empty_range_has_no_rects() {
        let mut fs = FontSystem::new();
        let paginated = paginate_blocks(&mut fs, &blocks_with(&["x"]), &cfg());
        let range = TextRange::new(TextAnchor::new(0, 0), TextAnchor::new(0, 0));
        assert!(paginated.rects_for_range(0, &range).is_empty());
    }

    #[test]
    fn cross_block_extract() {
        let blocks = blocks_with(&["alpha", "bravo"]);
        let range = TextRange::new(TextAnchor::new(0, 2), TextAnchor::new(1, 3));
        let text = extract_plain_text(&blocks, &range);
        assert_eq!(text, "pha\nbra");
    }
}
