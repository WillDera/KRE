//! Text layout: shapes KIR blocks with cosmic-text into positioned glyphs,
//! then paginates them into pages.
//!
//! Layout is independent of rendering: this module produces neutral,
//! positioned glyph lists that any backend can rasterize. Word wrapping,
//! line metrics, shaping (including per-script shaping), pagination, and
//! hit testing happen here. The layout layer retains the source byte range
//! of every glyph so selection and highlighting operate on the text layer,
//! never on raster pixels (OCR is never required).

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
    /// A page must not end or begin with fewer than this many lines of a
    /// split paragraph (widow/orphan control). Enforced as at least 1.
    pub min_lines: u32,
    /// Keep a heading on the same page as the block that follows it.
    pub keep_headings_with_next: bool,
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
            min_lines: 2,
            keep_headings_with_next: true,
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
    /// Source byte range `[start, end)` into the shaped text. Grounds
    /// hit testing and selection in the content layer.
    pub byte_range: (u32, u32),
}

/// One laid-out line of placed glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLine {
    pub glyphs: Vec<PlacedGlyph>,
    /// Device-pixel line height this line was shaped with.
    pub line_height: f32,
}

/// One laid-out page: positioned lines plus the source block each line came
/// from. Line y coordinates restart at the page margin.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub lines: Vec<PlacedLine>,
    /// `lines[i]` originates from block `line_blocks[i]` (index into the
    /// input `blocks` slice).
    pub line_blocks: Vec<usize>,
}

/// A chapter laid out into pages. Deterministic: identical inputs produce
/// identical pages.
#[derive(Debug, Clone, PartialEq)]
pub struct PaginatedLayout {
    pub pages: Vec<Page>,
}

impl PaginatedLayout {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Map a point within `page` (frame coordinates) to the text under it.
    /// Returns the source block index and the byte range of the glyph hit.
    ///
    /// Selection and highlighting are computed here against the layout layer,
    /// never against rasterized pixels.
    pub fn hit_test(&self, page: usize, x: f32, y: f32) -> Option<TextHit> {
        let page = self.pages.get(page)?;
        for (li, line) in page.lines.iter().enumerate() {
            let first = line.glyphs.first()?;
            let top = first.y as f32;
            if y < top || y >= top + line.line_height {
                continue;
            }
            for (gi, g) in line.glyphs.iter().enumerate() {
                let gx = g.x as f32;
                let next_x = line
                    .glyphs
                    .get(gi + 1)
                    .map(|n| n.x as f32)
                    .unwrap_or(gx + g.font_size);
                if x >= gx && x < next_x {
                    return Some(TextHit {
                        block: page.line_blocks[li],
                        start: g.byte_range.0,
                        end: g.byte_range.1,
                    });
                }
            }
            // Point past the last glyph on the line: an insertion point at
            // the line end.
            if let Some(last) = line.glyphs.last() {
                if x >= last.x as f32 {
                    return Some(TextHit {
                        block: page.line_blocks[li],
                        start: last.byte_range.1,
                        end: last.byte_range.1,
                    });
                }
            }
        }
        None
    }
}

/// A text position produced by [`PaginatedLayout::hit_test`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextHit {
    /// Index into the source `blocks` slice.
    pub block: usize,
    /// Start byte offset into the block's plain text.
    pub start: u32,
    /// End byte offset (exclusive) into the block's plain text.
    pub end: u32,
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
    let mut lines = Vec::new();
    let mut y = cfg.margin;
    for (index, block) in blocks.iter().enumerate() {
        let shaped = shape_block(font_system, index, block, cfg);
        if shaped.lines.is_empty() {
            continue;
        }
        for line in shaped.lines {
            let mut line = line;
            for g in &mut line.glyphs {
                g.y = y as i32;
            }
            y += line.line_height;
            lines.push(line);
        }
        y += shaped.paragraph_spacing;
    }
    lines
}

/// Paginate a batch of KIR blocks into pages sized by `cfg.width`/`cfg.height`.
///
/// Guarantees:
/// - a block never splits across a page boundary unless it is taller than a
///   full page (block integrity: quotes stay together);
/// - a page never ends or begins with fewer than `min_lines` of a split
///   paragraph (widow/orphan control);
/// - a heading stays on the same page as the block that follows it when
///   `keep_headings_with_next` is set;
/// - paragraph spacing is only counted between blocks, never after the last
///   block on a page.
pub fn paginate_blocks(
    font_system: &mut FontSystem,
    blocks: &[Block],
    cfg: &LayoutConfig,
) -> PaginatedLayout {
    let page_height = cfg.height as f32 - 2.0 * cfg.margin;
    let min_lines = cfg.min_lines.max(1) as usize;
    let margin = cfg.margin;
    // Small epsilon so floating-point line heights don't cascade page breaks.
    let eps = 0.5;

    let shaped: Vec<ShapedBlock> = blocks
        .iter()
        .enumerate()
        .map(|(idx, b)| shape_block(font_system, idx, b, cfg))
        .collect();

    let mut pages = Vec::new();
    let mut page = PageBuilder::new();

    let mut i = 0;
    while i < shaped.len() {
        let sb = &shaped[i];
        let n = sb.lines.len();
        let block_height = n as f32 * sb.line_height;
        let cost = block_height
            + if page.is_empty() {
                0.0
            } else {
                sb.paragraph_spacing
            };

        if n == 0 {
            i += 1;
            continue;
        }

        if cost <= page_height - page.used + eps {
            page.place(sb, 0, n, margin, true);
            i += 1;
            continue;
        }

        if block_height <= page_height + eps {
            // Atomic block: it moves whole to a fresh page. Before finishing
            // the current page, pull forward any trailing content that must
            // not be stranded there (keep-together overrides).
            let mut carry = None;
            if !page.is_empty() {
                let last_block = *page.blocks.last().unwrap();
                let last = &shaped[last_block];
                let on_page = page.blocks.iter().filter(|&&b| b == last_block).count();
                if cfg.keep_headings_with_next && last.is_heading && on_page == last.lines.len() {
                    // Heading with nothing following on this page: keep it
                    // with the block we are about to place.
                    carry = Some(page.pop_trailing(on_page));
                } else if !last.is_heading && on_page < min_lines && last.lines.len() > on_page {
                    // Widow: fewer than `min_lines` at the page bottom.
                    carry = Some(page.pop_trailing(on_page));
                }
            }
            page.finish(&mut pages);
            if let Some((lines, blocks)) = carry {
                page.replace_top(lines, blocks, margin);
            }
            page.place(sb, 0, n, margin, false);
            i += 1;
            continue;
        }

        // Block taller than a full page: split it at line boundaries. Every
        // page of the split (except the last) holds at least `min_lines`.
        let mut placed = 0;
        let mut first_chunk = true;
        while placed < n {
            let available = page_height - page.used;
            let capacity = ((available / sb.line_height).floor() as usize).max(1);
            let remaining = n - placed;

            // Don't start a split with a sub-minimum chunk on a nearly full
            // page; move the block up to a fresh page.
            if first_chunk && !page.is_empty() && capacity < min_lines && remaining > capacity {
                page.finish(&mut pages);
                continue;
            }

            // Take what fits, but avoid leaving a stranded final line: when
            // the paragraph ends on the next page with fewer than `min_lines`,
            // pull lines back — provided the current page still holds
            // `min_lines` of its own (otherwise the geometry allows no
            // better break).
            let mut take = remaining.min(capacity);
            let after = remaining - take;
            if after > 0 && after < min_lines && take > min_lines {
                let pull_back = (min_lines - after).min(take - min_lines);
                take -= pull_back;
            }
            take = take.max(1);

            page.place(sb, placed, placed + take, margin, first_chunk);
            placed += take;
            first_chunk = false;
            if placed < n {
                page.finish(&mut pages);
            }
        }
        i += 1;
    }
    page.finish(&mut pages);

    if pages.is_empty() {
        pages.push(Page {
            lines: Vec::new(),
            line_blocks: Vec::new(),
        });
    }
    PaginatedLayout { pages }
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

/// A block shaped into lines, ready for placement into pages. Glyph x
/// positions include the margin; y positions are relative to the block top
/// (placement overrides them with page-absolute coordinates).
struct ShapedBlock {
    block_index: usize,
    lines: Vec<PlacedLine>,
    line_height: f32,
    paragraph_spacing: f32,
    is_heading: bool,
}

fn shape_block(
    font_system: &mut FontSystem,
    block_index: usize,
    block: &Block,
    cfg: &LayoutConfig,
) -> ShapedBlock {
    let (metrics, paragraph_spacing) = block_metrics(block, cfg);
    let color = block_color(block, cfg);
    let mut lines = Vec::new();

    let text = block.plain_text();
    if !text.is_empty() {
        let attrs = match &cfg.font_family {
            Some(family) => Attrs::new().family(Family::Name(family)),
            None => Attrs::new(),
        };
        let text_width = cfg.width as f32 - 2.0 * cfg.margin;
        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_size(font_system, Some(text_width), None);
        buffer.set_wrap(font_system, cfg.wrap);
        buffer.set_text(font_system, &text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);

        for run in buffer.layout_runs() {
            let mut glyphs = Vec::with_capacity(run.glyphs.len());
            for glyph in run.glyphs {
                let physical = glyph.physical((cfg.margin, 0.0), 1.0);
                glyphs.push(PlacedGlyph {
                    font_id: glyph.font_id,
                    glyph_id: glyph.glyph_id,
                    font_size: glyph.font_size,
                    x: physical.x,
                    y: physical.y,
                    offset_x: physical.cache_key.x_bin.as_float(),
                    offset_y: physical.cache_key.y_bin.as_float(),
                    color,
                    byte_range: (glyph.start as u32, glyph.end as u32),
                });
            }
            if !glyphs.is_empty() {
                lines.push(PlacedLine {
                    glyphs,
                    line_height: metrics.line_height,
                });
            }
        }
    }

    ShapedBlock {
        block_index,
        lines,
        line_height: metrics.line_height,
        paragraph_spacing,
        is_heading: matches!(block.kind, Some(koma_core::kir::block::Kind::Heading(_))),
    }
}

/// In-progress page during pagination.
struct PageBuilder {
    lines: Vec<PlacedLine>,
    blocks: Vec<usize>,
    used: f32,
}

impl PageBuilder {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            blocks: Vec::new(),
            used: 0.0,
        }
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Place `shaped.lines[start..end]` on this page, advancing the used
    /// height. `with_spacing` applies the block's paragraph spacing when the
    /// page already holds content.
    fn place(
        &mut self,
        shaped: &ShapedBlock,
        start: usize,
        end: usize,
        margin: f32,
        with_spacing: bool,
    ) {
        if with_spacing && !self.lines.is_empty() {
            self.used += shaped.paragraph_spacing;
        }
        for line in &shaped.lines[start..end] {
            let y = margin + self.used;
            let mut line = line.clone();
            for g in &mut line.glyphs {
                g.y = y as i32;
            }
            self.blocks.push(shaped.block_index);
            self.lines.push(line);
            self.used += self.lines.last().unwrap().line_height;
        }
    }

    /// Remove the last `count` lines (and their block mapping), returning
    /// them with the page height they occupied subtracted.
    fn pop_trailing(&mut self, count: usize) -> (Vec<PlacedLine>, Vec<usize>) {
        let split = self.lines.len() - count;
        let lines = self.lines.split_off(split);
        let blocks = self.blocks.split_off(split);
        let height: f32 = lines.iter().map(|l| l.line_height).sum();
        self.used -= height;
        (lines, blocks)
    }

    /// Place carried lines at the top of this (fresh) page.
    fn replace_top(&mut self, lines: Vec<PlacedLine>, blocks: Vec<usize>, margin: f32) {
        for (line, block) in lines.into_iter().zip(blocks) {
            let y = margin + self.used;
            let mut line = line;
            for g in &mut line.glyphs {
                g.y = y as i32;
            }
            self.blocks.push(block);
            self.lines.push(line);
            self.used += self.lines.last().unwrap().line_height;
        }
    }

    /// Flush this page into `pages` and reset.
    fn finish(&mut self, pages: &mut Vec<Page>) {
        if !self.lines.is_empty() {
            pages.push(Page {
                lines: std::mem::take(&mut self.lines),
                line_blocks: std::mem::take(&mut self.blocks),
            });
        }
        self.used = 0.0;
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

    fn cfg(width: u32, height: u32) -> LayoutConfig {
        LayoutConfig {
            width,
            height,
            margin: 10.0,
            font_size: 16.0,
            line_height: 22.0,
            paragraph_spacing: 4.0,
            ..Default::default()
        }
    }

    fn heading(text: &str) -> Block {
        Block {
            kind: Some(koma_core::kir::block::Kind::Heading(
                koma_core::kir::Heading {
                    level: 1,
                    spans: vec![koma_core::kir::TextSpan {
                        text: text.to_owned(),
                        language: None,
                        style: None,
                    }],
                },
            )),
        }
    }

    #[test]
    fn wraps_long_paragraph_into_multiple_lines() {
        let mut fs = FontSystem::new();
        let long = "one two three four five six seven eight nine ten";
        let lines = layout_blocks(&mut fs, &blocks_with(&[long]), &cfg(300, 600));
        assert!(
            lines.len() >= 2,
            "expected wrapping, got {} lines",
            lines.len()
        );
        // Every glyph sits inside the frame width.
        for line in &lines {
            for g in &line.glyphs {
                assert!(g.x >= 0 && g.x < 300);
            }
        }
    }

    #[test]
    fn short_text_stays_on_one_line_and_layout_is_monotonic() {
        let mut fs = FontSystem::new();
        let lines = layout_blocks(&mut fs, &blocks_with(&["A cold coming."]), &cfg(800, 400));
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].glyphs.is_empty());

        // Two blocks stack vertically: first line y < second line y.
        let two = layout_blocks(&mut fs, &blocks_with(&["alpha", "bravo"]), &cfg(800, 400));
        assert_eq!(two.len(), 2);
        let first_y = two[0].glyphs.iter().map(|g| g.y).min().unwrap();
        let second_y = two[1].glyphs.iter().map(|g| g.y).min().unwrap();
        assert!(second_y > first_y);
    }

    #[test]
    fn cjk_text_shapes_to_nonempty_glyphs() {
        let mut fs = FontSystem::new();
        let lines = layout_blocks(
            &mut fs,
            &blocks_with(&["冰封世界的漫长冬日"]),
            &cfg(800, 200),
        );
        assert_eq!(lines.len(), 1, "CJK line should not wrap in a wide frame");
        assert!(!lines[0].glyphs.is_empty(), "CJK should shape to glyphs");
    }

    #[test]
    fn headings_scale_up_and_use_heading_color() {
        let mut fs = FontSystem::new();
        let c = cfg(800, 400);
        let blocks = vec![heading("Chapter One"), {
            Block {
                kind: Some(koma_core::kir::block::Kind::Paragraph(
                    koma_core::kir::paragraph("body text"),
                )),
            }
        }];
        let lines = layout_blocks(&mut fs, &blocks, &c);
        assert_eq!(lines.len(), 2);
        let heading_size = lines[0].glyphs.iter().map(|g| g.font_size).next().unwrap();
        let body_size = lines[1].glyphs.iter().map(|g| g.font_size).next().unwrap();
        assert!(heading_size > body_size, "heading should scale up");
        assert_eq!(lines[0].glyphs[0].color, c.heading_color);
        assert_eq!(lines[1].glyphs[0].color, c.text_color);
    }

    #[test]
    fn named_font_family_degrades_gracefully() {
        let mut fs = FontSystem::new();
        let c = LayoutConfig {
            width: 600,
            height: 100,
            font_family: Some("Definitely-Not-A-Real-Font".to_owned()),
            ..Default::default()
        };
        let lines = layout_blocks(&mut fs, &blocks_with(&["readable"]), &c);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].glyphs.is_empty());
    }

    #[test]
    fn glyphs_carry_source_byte_ranges() {
        let mut fs = FontSystem::new();
        let text = "A cold coming.";
        let lines = layout_blocks(&mut fs, &blocks_with(&[text]), &cfg(800, 400));
        let g0 = lines[0].glyphs[0];
        assert_eq!(g0.byte_range.0, 0);
        let first = g0.byte_range.0 as usize;
        let last = lines[0].glyphs.last().unwrap().byte_range.1 as usize;
        assert_eq!(&text[first..last], "A cold coming.");
        // Ranges are ordered and non-empty.
        for w in lines[0].glyphs.windows(2) {
            assert!(w[0].byte_range.1 <= w[1].byte_range.1);
        }
    }

    #[test]
    fn blocks_fill_pages_without_splitting() {
        let mut fs = FontSystem::new();
        // Page holds exactly two one-line blocks (2*22 + one 4px spacing).
        let c = cfg(300, 90);
        let blocks = blocks_with(&["alpha", "beta", "gamma", "delta"]);
        let paginated = paginate_blocks(&mut fs, &blocks, &c);
        assert_eq!(paginated.page_count(), 2, "two blocks per page");
        let per_page: Vec<Vec<usize>> = paginated
            .pages
            .iter()
            .map(|p| p.line_blocks.to_vec())
            .collect();
        assert_eq!(per_page, vec![vec![0, 1], vec![2, 3]]);
        // No block is split: line count matches block count on every page.
        for page in &paginated.pages {
            assert_eq!(page.line_blocks.len(), page.lines.len());
        }
    }

    #[test]
    fn quote_block_is_never_split_across_pages() {
        let mut fs = FontSystem::new();
        let quote = Block {
            kind: Some(koma_core::kir::block::Kind::Quote(
                koma_core::kir::QuoteBlock {
                    content: vec![Block {
                        kind: Some(koma_core::kir::block::Kind::Paragraph(
                            koma_core::kir::paragraph(
                                "one two three four five six seven eight nine ten eleven",
                            ),
                        )),
                    }],
                    attribution: None,
                },
            )),
        };
        // Determine the quote's height at this width, then give the page just
        // enough room for the intro block but not the quote alongside it.
        let tall = cfg(160, 1000);
        let q_lines = paginate_blocks(&mut fs, std::slice::from_ref(&quote), &tall).pages[0]
            .lines
            .len();
        let quote_height = q_lines as f32 * 22.0;
        assert!(quote_height <= 90.0, "test geometry assumes atomic quote");
        let page_height = quote_height + 10.0; // between quote_h and quote_h+intro+spacing
        let c = LayoutConfig {
            height: page_height as u32 + 20,
            ..cfg(160, 100)
        };
        let blocks = vec![blocks_with(&["intro"])[0].clone(), quote];
        let paginated = paginate_blocks(&mut fs, &blocks, &c);
        assert_eq!(paginated.page_count(), 2, "intro then quote");
        assert_eq!(paginated.pages[0].line_blocks, vec![0], "intro on page 1");
        assert_eq!(
            paginated.pages[1].line_blocks,
            vec![1; q_lines],
            "quote whole on page 2"
        );
    }

    #[test]
    fn heading_is_kept_with_following_block() {
        let mut fs = FontSystem::new();
        let a_text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
        let blocks = vec![
            blocks_with(&[a_text])[0].clone(),                      // 0: filler
            heading("Chapter"),                                     // 1: heading
            blocks_with(&["body follows the heading."])[0].clone(), // 2: body
        ];
        // Page height = filler + heading + spacing: the heading would fit at
        // the bottom of page 1, but the body would not — the keep-with-next
        // scenario.
        let a_lines = layout_blocks(&mut fs, &[blocks[0].clone()], &cfg(300, 400)).len();
        let page_height = a_lines as f32 * 22.0 + 33.0 + 4.0;
        let c = LayoutConfig {
            height: page_height as u32 + 20,
            ..cfg(300, 400)
        };

        let kept = paginate_blocks(&mut fs, &blocks, &c);
        let heading_page = kept
            .pages
            .iter()
            .position(|p| p.line_blocks.contains(&1))
            .unwrap();
        let body_page = kept
            .pages
            .iter()
            .position(|p| p.line_blocks.contains(&2))
            .unwrap();
        assert_eq!(heading_page, body_page, "heading kept with following block");

        let c = LayoutConfig {
            keep_headings_with_next: false,
            ..c
        };
        let not_kept = paginate_blocks(&mut fs, &blocks, &c);
        let heading_page = not_kept
            .pages
            .iter()
            .position(|p| p.line_blocks.contains(&1))
            .unwrap();
        let body_page = not_kept
            .pages
            .iter()
            .position(|p| p.line_blocks.contains(&2))
            .unwrap();
        assert_ne!(
            heading_page, body_page,
            "without the rule the heading strands"
        );
    }

    #[test]
    fn split_pages_hold_min_lines_and_preserve_all_lines() {
        let mut fs = FontSystem::new();
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three";
        let blocks = blocks_with(&[long]);
        // Page fits 2 body lines.
        let c = cfg(160, 66);
        let single = layout_blocks(&mut fs, &blocks, &c);
        assert!(
            single.len() > 4,
            "test geometry assumes a multi-page paragraph"
        );
        let paginated = paginate_blocks(&mut fs, &blocks, &c);
        assert!(
            paginated.page_count() >= 3,
            "paragraph must span multiple pages"
        );

        let total: usize = paginated.pages.iter().map(|p| p.lines.len()).sum();
        assert_eq!(
            total,
            single.len(),
            "no lines lost or duplicated across the split"
        );
        for (pi, page) in paginated.pages.iter().enumerate() {
            let is_last = pi == paginated.pages.len() - 1;
            let n = page.lines.len();
            if !is_last {
                assert!(n >= 2, "page {pi} holds {n} lines (< min_lines)");
            }
        }
        // Every line still maps back to block 0.
        for page in &paginated.pages {
            assert!(page.line_blocks.iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn hit_test_returns_block_and_byte_range() {
        let mut fs = FontSystem::new();
        let text = "The old inquisitor";
        let blocks = blocks_with(&[text]);
        let c = cfg(800, 400);
        let paginated = paginate_blocks(&mut fs, &blocks, &c);
        let page = &paginated.pages[0];
        let line = &page.lines[0];
        let first = line.glyphs[0];
        let hit = paginated
            .hit_test(0, first.x as f32 + 1.0, first.y as f32 + 2.0)
            .expect("hit on first glyph");
        assert_eq!(hit.block, 0);
        assert_eq!(hit.start, 0);
        assert_eq!(&text[hit.start as usize..hit.end as usize], "T");
        // Point below all text misses.
        assert!(paginated.hit_test(0, 4.0, 1000.0).is_none());
    }

    #[test]
    fn hit_test_out_of_bounds_page_returns_none() {
        let mut fs = FontSystem::new();
        let paginated = paginate_blocks(&mut fs, &blocks_with(&["x"]), &cfg(400, 200));
        assert!(paginated.hit_test(5, 0.0, 0.0).is_none());
    }
}
