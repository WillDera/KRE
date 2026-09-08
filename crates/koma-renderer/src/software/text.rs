//! Glyph rasterization for the software backend (swash).
//!
//! Uses swash to render shaped glyphs (from cosmic-text) into the RGBA frame
//! with alpha blending. This is the CPU fallback path; wgpu (Phase 7) will
//! replace it on GPU-capable hosts while keeping this for portability and
//! tests.

use cosmic_text::{Attrs, FontSystem, Metrics, Shaping, Wrap};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Format, Vector};

use crate::backend::{Color, Frame};
use crate::layout::PlacedGlyph;

/// Rasterize a placed glyph into `frame`. `glyph.x/y` are absolute frame
/// pixel coordinates (top-left of the glyph bounding box).
pub fn rasterize_glyph(font_system: &FontSystem, frame: &mut Frame, glyph: PlacedGlyph) {
    font_system
        .db()
        .with_face_data(glyph.font_id, |data, index| {
            let Some(font) = FontRef::from_index(data, index as usize) else {
                return;
            };
            rasterize(&font, frame, glyph);
        });
}

fn rasterize(font: &FontRef, frame: &mut Frame, glyph: PlacedGlyph) {
    // Size in pixels per em; swash renders at device-pixel resolution.
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(*font).size(glyph.font_size).build();

    // Align the glyph bitmap: sub-pixel offsets in x, and the inverse offset
    // in y (cosmic-text and swash differ in y direction).
    let offset_x = glyph.x as f32 + glyph.offset_x;
    let offset_y = glyph.y as f32 - glyph.offset_y;

    let mut render = Render::new(&[Source::Outline]);
    render
        .format(Format::Alpha)
        .offset(Vector::new(offset_x, offset_y));

    let Some(image) = render.render(&mut scaler, glyph.glyph_id) else {
        return;
    };
    let w = image.placement.width as usize;
    let h = image.placement.height as usize;
    for y in 0..h {
        for x in 0..w {
            let alpha = image.data[y * w + x];
            if alpha > 0 {
                let px = image.placement.left + x as i32;
                let py = image.placement.top + y as i32;
                let a = (alpha as u32 * glyph.color.a as u32 / 255) as u8;
                frame.blend_pixel(px, py, Color { a, ..glyph.color });
            }
        }
    }
}

/// Rasterize a free-form text item (render-primitives path) at `x`/`y`.
pub fn draw_text_item(
    font_system: &mut FontSystem,
    frame: &mut Frame,
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    color: Color,
) {
    let metrics = Metrics {
        font_size,
        line_height: font_size * 1.4,
    };
    let mut buffer = cosmic_text::Buffer::new(font_system, metrics);
    buffer.set_size(
        font_system,
        Some(frame.width as f32 - x),
        Some(frame.height as f32 - y),
    );
    buffer.set_wrap(font_system, Wrap::Word);
    buffer.set_text(font_system, text, Attrs::new(), Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let physical = glyph.physical((x, y), 1.0);
            rasterize_glyph(
                font_system,
                frame,
                PlacedGlyph {
                    font_id: glyph.font_id,
                    glyph_id: glyph.glyph_id,
                    font_size: glyph.font_size,
                    x: physical.x,
                    y: physical.y,
                    offset_x: physical.cache_key.x_bin.as_float(),
                    offset_y: physical.cache_key.y_bin.as_float(),
                    color,
                    byte_range: (glyph.start as u32, glyph.end as u32),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use cosmic_text::FontSystem;

    use super::*;

    #[test]
    fn rasterizes_text_into_non_empty_frame() {
        let mut fs = FontSystem::new();
        let mut frame = Frame::new(300, 80);
        draw_text_item(
            &mut fs,
            &mut frame,
            "Hello Koma",
            4.0,
            4.0,
            24.0,
            Color::BLACK,
        );
        let opaque = frame.pixels.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(opaque > 0, "expected some opaque pixels, got {opaque}");
    }
}
