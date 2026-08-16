//! CPU-side glyph atlas with shelf packing.
//!
//! Glyphs are rasterized on the CPU (swash), packed into a growing RGBA
//! texture, and uploaded to the GPU once per frame. Each glyph occupies one
//! atlas cell with a 1px padding shelf to avoid bleeding under nearest
//! sampling.

use wgpu::{Device, Queue, Texture, TextureFormat};

/// A region of the atlas holding one glyph bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Max atlas dimension; glyphs larger than this are skipped (rare for text).
const MAX_DIM: u32 = 8192;

/// A growing glyph atlas. Old glyphs stay at their offsets when the atlas
/// grows, so upload happens once at the end of the frame.
pub struct GlyphAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    pub fn new() -> Self {
        let width = 512;
        let height = 512;
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pack a `w*h` alpha coverage bitmap; returns its atlas rect.
    pub fn insert(&mut self, w: u32, h: u32, coverage: &[u8]) -> Option<AtlasRect> {
        if w == 0 || h == 0 || coverage.len() != (w * h) as usize {
            return None;
        }
        // 1px padding between glyphs.
        let pw = w + 1;
        let ph = h + 1;
        if pw > MAX_DIM || ph > MAX_DIM {
            return None;
        }
        loop {
            if self.cursor_x + pw <= self.width && self.cursor_y + ph <= self.height {
                break;
            }
            // Close the current shelf and try the next row.
            if self.cursor_x != 0 {
                self.cursor_x = 0;
                self.cursor_y += self.row_height;
                self.row_height = 0;
            }
            // Grow until the glyph fits in both dimensions.
            if self.cursor_x + pw > self.width {
                self.grow();
            }
            if self.cursor_y + ph > self.height {
                self.grow();
            }
        }
        let x = self.cursor_x;
        let y = self.cursor_y;
        for gy in 0..h {
            for gx in 0..w {
                let i = ((y + gy) * self.width + (x + gx)) as usize * 4;
                self.pixels[i] = 255;
                self.pixels[i + 1] = 255;
                self.pixels[i + 2] = 255;
                self.pixels[i + 3] = coverage[(gy * w + gx) as usize];
            }
        }
        self.cursor_x = x + pw;
        self.row_height = self.row_height.max(ph);
        Some(AtlasRect { x, y, w, h })
    }

    fn grow(&mut self) {
        let nw = (self.width * 2).min(MAX_DIM);
        let nh = (self.height * 2).min(MAX_DIM);
        let mut next = vec![0; (nw * nh * 4) as usize];
        for y in 0..self.height {
            let src = (y * self.width) as usize * 4;
            let dst = (y * nw) as usize * 4;
            next[dst..dst + (self.width * 4) as usize]
                .copy_from_slice(&self.pixels[src..src + (self.width * 4) as usize]);
        }
        self.width = nw;
        self.height = nh;
        self.pixels = next;
    }

    /// Create the GPU texture and upload the atlas.
    pub fn upload(&self, device: &Device, queue: &Queue) -> Texture {
        let size = wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("koma glyph atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            size,
        );
        texture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_grows() {
        let mut atlas = GlyphAtlas::new();
        let a = atlas.insert(8, 10, &[7u8; 80]).expect("first glyph");
        assert_eq!(a.w, 8);
        assert_eq!(a.h, 10);
        assert_eq!(a.x, 0);
        assert_eq!(a.y, 0);

        // A glyph wider than the initial shelf grows the atlas.
        let b = atlas.insert(700, 20, &[9u8; 700 * 20]).expect("grew glyph");
        assert!(atlas.width() > 512, "atlas should have grown");
        assert!(b.w == 700);
        assert!(b.x + b.w <= atlas.width());

        // Pixels written at the first rect.
        let i = (a.y * atlas.width() + a.x) as usize * 4;
        assert_eq!(atlas.pixels[i + 3], 7);
    }

    #[test]
    fn oversized_glyph_is_rejected() {
        let mut atlas = GlyphAtlas::new();
        assert!(atlas.insert(MAX_DIM * 2, 4, &[0u8; 8]).is_none());
    }
}
