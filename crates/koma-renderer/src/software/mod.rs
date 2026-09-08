//! Software (CPU) rendering backend.
//!
//! Phase 4 backend: deterministic, portable, testable. Renders shaped text
//! (and bitmaps) into RGBA frames. GPU-accelerated effects (particles,
//! shaders, animation) are not supported here and degrade to static output
//! per AGENTS.md failure-handling rules.

pub mod text;

use cosmic_text::FontSystem;
use koma_core::kir::Block;

use crate::backend::{BackendError, Capabilities, Frame, RenderBackend, RenderPrimitive};
use crate::layout::{LayoutConfig, layout_blocks, paginate_blocks};
use crate::selection::{UserMark, paint_rects};

/// CPU rasterizer implementing [`RenderBackend`].
pub struct SoftwareBackend {
    font_system: FontSystem,
    capabilities: Capabilities,
}

impl SoftwareBackend {
    /// Backend using the system-installed fonts.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            capabilities: Capabilities {
                gpu: false,
                effects: false,
                animation: false,
            },
        }
    }

    /// Backend using an explicit font database (deterministic for tests and
    /// reproducible rendering).
    pub fn with_font_database(db: fontdb::Database) -> Self {
        Self {
            font_system: FontSystem::new_with_locale_and_db("en".to_string(), db),
            capabilities: Capabilities {
                gpu: false,
                effects: false,
                animation: false,
            },
        }
    }

    /// Render KIR blocks into a frame using `cfg`. Convenience entry point
    /// used by the CLI and tests.
    pub fn render_blocks(
        &mut self,
        blocks: &[Block],
        cfg: &LayoutConfig,
    ) -> Result<Frame, BackendError> {
        let lines = layout_blocks(&mut self.font_system, blocks, cfg);
        let mut frame = Frame::new(cfg.width, cfg.height);
        frame.fill(cfg.background);
        for line in &lines {
            for g in &line.glyphs {
                text::rasterize_glyph(&self.font_system, &mut frame, *g);
            }
        }
        Ok(frame)
    }

    /// Paginate `blocks` and render every page into its own frame.
    pub fn render_paginated(
        &mut self,
        blocks: &[Block],
        cfg: &LayoutConfig,
    ) -> Result<Vec<Frame>, BackendError> {
        self.render_paginated_with_marks(blocks, cfg, &[])
    }

    /// Paginate and render, painting user-mark underlays (highlights) before
    /// glyphs. Marks are user state — they never mutate KIR content.
    pub fn render_paginated_with_marks(
        &mut self,
        blocks: &[Block],
        cfg: &LayoutConfig,
        marks: &[UserMark],
    ) -> Result<Vec<Frame>, BackendError> {
        let paginated = paginate_blocks(&mut self.font_system, blocks, cfg);
        let mut frames = Vec::with_capacity(paginated.page_count());
        for (page_idx, page) in paginated.pages.iter().enumerate() {
            let mut frame = Frame::new(cfg.width, cfg.height);
            frame.fill(cfg.background);
            for mark in marks {
                if matches!(mark.kind, crate::selection::MarkKind::Highlight)
                    || mark.color.is_some()
                {
                    let rects = paginated.rects_for_range(page_idx, &mark.range);
                    paint_rects(&mut frame, &rects, mark.resolve_color());
                }
            }
            for deco in &page.decorations {
                frame.fill_rect(
                    deco.x as i32,
                    deco.y as i32,
                    deco.width.ceil() as i32,
                    deco.height.ceil().max(1.0) as i32,
                    deco.color,
                );
            }
            for line in &page.lines {
                for g in &line.glyphs {
                    text::rasterize_glyph(&self.font_system, &mut frame, *g);
                }
            }
            frames.push(frame);
        }
        Ok(frames)
    }
}

impl Default for SoftwareBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for SoftwareBackend {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn draw(&mut self, primitives: &[RenderPrimitive]) -> Result<Frame, BackendError> {
        // Compute a bounding frame from the primitives so callers get a
        // sensible result without specifying size.
        let (mut width, mut height) = (0u32, 0u32);
        for p in primitives {
            match p {
                RenderPrimitive::Image(img) => {
                    width = width.max(img.x as u32 + img.width);
                    height = height.max(img.y as u32 + img.height);
                }
                RenderPrimitive::Text(t) => {
                    width = width.max(t.x as u32 + t.text.len() as u32 * (t.font_size as u32 / 2));
                    height = height.max(t.y as u32 + t.font_size as u32);
                }
                _ => {}
            }
        }
        let (width, height) = (width.max(1), height.max(1));
        let mut frame = Frame::new(width, height);

        for p in primitives {
            match p {
                RenderPrimitive::Text(t) => text::draw_text_item(
                    &mut self.font_system,
                    &mut frame,
                    &t.text,
                    t.x,
                    t.y,
                    t.font_size,
                    t.color,
                ),
                RenderPrimitive::Image(img) => blit_image(&mut frame, img),
                // Particles, shaders, and animation have no software
                // implementation yet: static fallback per AGENTS.md.
                _ => {}
            }
        }
        Ok(frame)
    }
}

fn blit_image(frame: &mut Frame, img: &crate::backend::ImageItem) {
    let sw = img.width as i32;
    let sh = img.height as i32;
    for sy in 0..sh {
        for sx in 0..sw {
            let src = ((sy * sw + sx) * 4) as usize;
            let color = crate::backend::Color {
                r: img.rgba[src],
                g: img.rgba[src + 1],
                b: img.rgba[src + 2],
                a: img.rgba[src + 3],
            };
            frame.blend_pixel(img.x as i32 + sx, img.y as i32 + sy, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_renders_blocks_to_frame() {
        let mut backend = SoftwareBackend::new();
        let cfg = LayoutConfig {
            width: 400,
            height: 200,
            ..Default::default()
        };
        let blocks = vec![Block {
            kind: Some(koma_core::kir::block::Kind::Paragraph(
                koma_core::kir::paragraph("The inquisitor entered the chamber."),
            )),
        }];
        let frame = backend.render_blocks(&blocks, &cfg).unwrap();
        assert_eq!(frame.width, 400);
        assert_eq!(frame.height, 200);
        let opaque = frame.pixels.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(opaque > 0);
    }

    #[test]
    fn paginated_marks_paint_highlight_underlay() {
        use crate::selection::{TextAnchor, TextRange, UserMark};

        let mut backend = SoftwareBackend::new();
        let cfg = LayoutConfig {
            width: 400,
            height: 200,
            margin: 10.0,
            font_size: 20.0,
            line_height: 28.0,
            ..Default::default()
        };
        let blocks = vec![Block {
            kind: Some(koma_core::kir::block::Kind::Paragraph(koma_core::kir::paragraph(
                "Highlight me please",
            ))),
        }];
        let plain = backend.render_paginated(&blocks, &cfg).unwrap();
        let marked = backend
            .render_paginated_with_marks(
                &blocks,
                &cfg,
                &[UserMark::highlight(
                    "h1",
                    TextRange::new(TextAnchor::new(0, 0), TextAnchor::new(0, 9)),
                )],
            )
            .unwrap();
        assert_eq!(plain.len(), marked.len());
        assert_ne!(
            plain[0].pixels, marked[0].pixels,
            "highlight underlay must change the frame"
        );
    }

    #[test]
    fn image_primitive_blits() {
        let mut backend = SoftwareBackend::new();
        let img = crate::backend::ImageItem {
            x: 2.0,
            y: 3.0,
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        };
        let frame = backend.draw(&[RenderPrimitive::Image(img)]).unwrap();
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 5);
        // Top-left pixel of the image is red at frame (2,3).
        let i = (3 * 4 + 2) * 4;
        assert_eq!(&frame.pixels[i..i + 4], &[255, 0, 0, 255]);
    }
}
