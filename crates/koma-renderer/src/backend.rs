//! Rendering backend abstraction.
//!
//! The engine defines rendering primitives here; concrete backends (software,
//! and later wgpu/Vulkan/Metal/WebGPU) implement [`RenderBackend`]. The scene
//! system depends only on this trait and its types — never on a specific GPU
//! API.

use thiserror::Error;

/// RGBA8 color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    fn channel(&self, index: usize) -> f32 {
        match index {
            0 => self.r as f32,
            1 => self.g as f32,
            _ => self.b as f32,
        }
    }
}

/// A run of text to draw at a position.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: Color,
    /// Per-span BCP-47 language override (shaping / bidi).
    pub language: Option<String>,
}

/// A bitmap image to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageItem {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    /// RGBA8 pixels, length `width * height * 4`.
    pub rgba: Vec<u8>,
}

/// Procedural particle effect (snow, rain, fog, ...). Declarative params.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleItem {
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub params: Vec<f32>,
}

/// A shader pass. Named; software backend falls back to static rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderItem {
    pub name: String,
    pub params: Vec<f32>,
}

/// An animation at a given timeline progress.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationItem {
    pub name: String,
    pub progress: f32,
}

/// A transform applied to the following primitives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformItem {
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
}

/// One drawable primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderPrimitive {
    Text(TextItem),
    Image(ImageItem),
    Particles(ParticleItem),
    Shader(ShaderItem),
    Animation(AnimationItem),
    Transform(TransformItem),
}

/// A raster frame: RGBA8 pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    /// Fill the whole frame with a color.
    pub fn fill(&mut self, color: Color) {
        for px in self.pixels.chunks_exact_mut(4) {
            px[0] = color.r;
            px[1] = color.g;
            px[2] = color.b;
            px[3] = color.a;
        }
    }

    /// Blend a single RGBA pixel, source-over.
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let dst = &mut self.pixels[i..i + 4];
        let sa = color.a as f32 / 255.0;
        let da = dst[3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        if oa <= 0.0 {
            return;
        }
        for (channel, dst_c) in dst.iter_mut().take(3).enumerate() {
            let s = color.channel(channel) * sa;
            let d = *dst_c as f32 * da * (1.0 - sa);
            *dst_c = ((s + d) / oa).round() as u8;
        }
        dst[3] = (oa * 255.0).round() as u8;
    }

    /// Fill an axis-aligned rectangle with source-over blending (highlight
    /// underlays, selection chrome).
    pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: Color) {
        if width <= 0 || height <= 0 {
            return;
        }
        let x1 = x + width;
        let y1 = y + height;
        for py in y.max(0)..y1.min(self.height as i32) {
            for px in x.max(0)..x1.min(self.width as i32) {
                self.blend_pixel(px, py, color);
            }
        }
    }
}

/// What a backend can do (used for feature negotiation / fallbacks).
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub gpu: bool,
    pub effects: bool,
    pub animation: bool,
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("render error: {0}")]
    Render(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A rendering backend. Scene code depends only on this interface.
pub trait RenderBackend {
    fn capabilities(&self) -> &Capabilities;

    /// Draw a batch of primitives into a frame.
    fn draw(&mut self, primitives: &[RenderPrimitive]) -> Result<Frame, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_fill_and_blend() {
        let mut f = Frame::new(2, 1);
        f.fill(Color::WHITE);
        assert_eq!(&f.pixels, &[255, 255, 255, 255, 255, 255, 255, 255]);

        f.blend_pixel(0, 0, Color::rgb(0, 0, 0));
        assert_eq!(&f.pixels[..4], &[0, 0, 0, 255]);

        // Out of bounds is a no-op.
        f.blend_pixel(10, 10, Color::rgb(1, 2, 3));
        assert_eq!(f.pixels.len(), 8);
    }
}
