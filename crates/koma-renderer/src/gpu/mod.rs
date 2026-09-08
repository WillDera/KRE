//! GPU (wgpu) rendering backend.
//!
//! Phase 7: a headless wgpu backend behind [`RenderBackend`]. It shapes text
//! (cosmic-text), rasterizes glyphs on the CPU (swash), packs them into a
//! glyph atlas texture, and composites them onto an offscreen render target
//! with a minimal WGSL pipeline. The target is read back into an RGBA8
//! [`Frame`], keeping the same output contract as the software backend.
//!
//! Scope (Big Dog decision, Phase 7): text and background are GPU-rendered;
//! particles, shaders, animation, and images degrade to the static fallback.
//! When no adapter/device is available the backend fails to construct, and
//! callers fall back to the software backend (AGENTS.md failure handling).

pub mod atlas;

use cosmic_text::{Attrs, FontSystem, Metrics, Shaping, Wrap};
use koma_core::kir::Block;
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Format, Vector};

use crate::backend::{BackendError, Capabilities, Frame, RenderBackend, RenderPrimitive};
use crate::layout::{LayoutConfig, PlacedGlyph, PlacedLine, layout_blocks, paginate_blocks};

use atlas::{AtlasRect, GlyphAtlas};

/// Minimal WGSL pipeline: textured quads, pixel-space positions.
const SHADER: &str = r#"
struct Globals {
    screen: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(
        pos.x / globals.screen.x * 2.0 - 1.0,
        1.0 - pos.y / globals.screen.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = uv;
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(atlas, atlas_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

/// One textured quad vertex.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// Per-frame uniform: screen size for the NDC transform.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    screen: [f32; 2],
}

/// Rasterized glyph bitmap with its device-pixel placement.
struct GlyphBitmap {
    left: i32,
    top: i32,
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

/// wgpu backend implementing [`RenderBackend`].
pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    font_system: FontSystem,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    capabilities: Capabilities,
}

impl WgpuBackend {
    /// Create a GPU backend using the system-installed fonts. Fails when no
    /// adapter/device is available; callers should fall back to software.
    pub fn new() -> Result<Self, BackendError> {
        Self::with_font_database(FontSystem::new().db().clone())
    }

    /// Create a GPU backend using an explicit font database.
    pub fn with_font_database(db: fontdb::Database) -> Result<Self, BackendError> {
        let (device, queue) = init_device()?;
        let pipeline = create_pipeline(&device);
        let bind_group_layout = create_bind_group_layout(&device);
        Ok(Self {
            device,
            queue,
            font_system: FontSystem::new_with_locale_and_db("en".to_string(), db),
            pipeline,
            bind_group_layout,
            capabilities: Capabilities {
                gpu: true,
                effects: false,
                animation: false,
            },
        })
    }

    /// Render KIR blocks into a frame using `cfg`. Mirrors
    /// [`crate::SoftwareBackend::render_blocks`].
    pub fn render_blocks(
        &mut self,
        blocks: &[Block],
        cfg: &LayoutConfig,
    ) -> Result<Frame, BackendError> {
        let lines = layout_blocks(&mut self.font_system, blocks, cfg);
        let (vertices, atlas) = self.build_quads(&lines)?;
        if vertices.is_empty() {
            let mut frame = Frame::new(cfg.width, cfg.height);
            frame.fill(cfg.background);
            return Ok(frame);
        }
        self.composite(cfg.width, cfg.height, cfg.background, &vertices, &atlas)
    }

    /// Paginate `blocks` and render every page into its own frame.
    pub fn render_paginated(
        &mut self,
        blocks: &[Block],
        cfg: &LayoutConfig,
    ) -> Result<Vec<Frame>, BackendError> {
        let paginated = paginate_blocks(&mut self.font_system, blocks, cfg);
        let mut frames = Vec::with_capacity(paginated.page_count());
        for page in &paginated.pages {
            let (vertices, atlas) = self.build_quads(&page.lines)?;
            if vertices.is_empty() {
                let mut frame = Frame::new(cfg.width, cfg.height);
                frame.fill(cfg.background);
                frames.push(frame);
            } else {
                frames.push(self.composite(
                    cfg.width,
                    cfg.height,
                    cfg.background,
                    &vertices,
                    &atlas,
                )?);
            }
        }
        Ok(frames)
    }

    /// Shape a free-form text item into placed glyphs (render-primitives path).
    fn shape_text_item(
        &mut self,
        item: &crate::backend::TextItem,
        width: u32,
        height: u32,
    ) -> Vec<PlacedLine> {
        let metrics = Metrics {
            font_size: item.font_size,
            line_height: item.font_size * 1.4,
        };
        let mut buffer = cosmic_text::Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            Some(width as f32 - item.x),
            Some(height as f32 - item.y),
        );
        buffer.set_wrap(&mut self.font_system, Wrap::Word);
        buffer.set_text(
            &mut self.font_system,
            &item.text,
            Attrs::new(),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut lines = Vec::new();
        for run in buffer.layout_runs() {
            let mut glyphs = Vec::with_capacity(run.glyphs.len());
            for glyph in run.glyphs {
                let physical = glyph.physical((item.x, item.y), 1.0);
                glyphs.push(PlacedGlyph {
                    font_id: glyph.font_id,
                    glyph_id: glyph.glyph_id,
                    font_size: glyph.font_size,
                    x: physical.x,
                    y: physical.y,
                    offset_x: physical.cache_key.x_bin.as_float(),
                    offset_y: physical.cache_key.y_bin.as_float(),
                    color: item.color,
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
        lines
    }

    /// Rasterize a single placed glyph with swash (same math as the software
    /// backend: subpixel x offset, inverted y offset).
    fn rasterize_glyph_bitmap(&self, glyph: &PlacedGlyph) -> Option<GlyphBitmap> {
        let mut out = None;
        self.font_system
            .db()
            .with_face_data(glyph.font_id, |data, index| {
                let Some(font) = FontRef::from_index(data, index as usize) else {
                    return;
                };
                let mut context = ScaleContext::new();
                let mut scaler = context.builder(font).size(glyph.font_size).build();
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
                if w == 0 || h == 0 {
                    return;
                }
                out = Some(GlyphBitmap {
                    left: image.placement.left,
                    top: image.placement.top,
                    width: w,
                    height: h,
                    coverage: image.data[..w * h].to_vec(),
                });
            });
        out
    }

    /// Rasterize all glyphs into a glyph atlas and build quad vertices.
    fn build_quads(
        &mut self,
        lines: &[PlacedLine],
    ) -> Result<(Vec<Vertex>, GlyphAtlas), BackendError> {
        let mut atlas = GlyphAtlas::new();
        let mut vertices = Vec::new();
        for line in lines {
            for glyph in &line.glyphs {
                let Some(bitmap) = self.rasterize_glyph_bitmap(glyph) else {
                    continue;
                };
                let Some(rect) =
                    atlas.insert(bitmap.width as u32, bitmap.height as u32, &bitmap.coverage)
                else {
                    continue;
                };
                push_quad(
                    &mut vertices,
                    bitmap.left,
                    bitmap.top,
                    rect,
                    &atlas,
                    glyph.color,
                );
            }
        }
        Ok((vertices, atlas))
    }

    /// Run the GPU pass: clear to background, draw quads, read back pixels.
    fn composite(
        &self,
        width: u32,
        height: u32,
        background: crate::backend::Color,
        vertices: &[Vertex],
        atlas: &GlyphAtlas,
    ) -> Result<Frame, BackendError> {
        let atlas_texture = atlas.upload(&self.device, &self.queue);
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("koma glyph sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let globals = Globals {
            screen: [width as f32, height as f32],
        };
        let globals_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("koma globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&globals_buf, 0, bytemuck::bytes_of(&globals));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("koma text bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &globals_buf,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let vertex_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("koma vertices"),
            size: std::mem::size_of_val(vertices) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&vertex_buf, 0, bytemuck::cast_slice(vertices));

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("koma target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

        // Staging buffer with wgpu row alignment for readback.
        let bytes_per_row = (width * 4).max(1);
        let aligned = align_up(
            bytes_per_row as u64,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64,
        ) as u32;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("koma staging"),
            size: aligned as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("koma encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("koma pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: background.r as f64 / 255.0,
                            g: background.g as f64 / 255.0,
                            b: background.b as f64 / 255.0,
                            a: background.a as f64 / 255.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buf.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        // Synchronous readback: map + poll until the map callback runs.
        let (tx, rx) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| BackendError::Render(format!("poll failed: {e}")))?;
        let map_result = rx
            .recv()
            .map_err(|_| BackendError::Render("map channel closed".into()))?;
        map_result.map_err(|e| BackendError::Render(format!("buffer map failed: {e}")))?;

        let mapped = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|e| BackendError::Render(format!("get_mapped_range failed: {e}")))?;
        let mut frame = Frame::new(width, height);
        for row in 0..height {
            let src = (row as usize) * aligned as usize;
            let dst = (row as usize) * (width as usize) * 4;
            frame.pixels[dst..dst + (width as usize * 4)]
                .copy_from_slice(&mapped[src..src + (width as usize * 4)]);
        }
        drop(mapped);
        staging.unmap();
        Ok(frame)
    }
}

impl RenderBackend for WgpuBackend {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn draw(&mut self, primitives: &[RenderPrimitive]) -> Result<Frame, BackendError> {
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

        let mut lines = Vec::new();
        for p in primitives {
            if let RenderPrimitive::Text(t) = p {
                lines.extend(self.shape_text_item(t, width, height));
            }
        }
        let (vertices, atlas) = self.build_quads(&lines)?;
        if vertices.is_empty() {
            let mut frame = Frame::new(width, height);
            frame.fill(crate::backend::Color::BLACK);
            return Ok(frame);
        }
        // Transparent background for the free-form primitives path.
        self.composite(
            width,
            height,
            crate::backend::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            &vertices,
            &atlas,
        )
    }
}

/// Push two triangles for one glyph quad, sampling texel centers.
fn push_quad(
    vertices: &mut Vec<Vertex>,
    x0: i32,
    y0: i32,
    rect: AtlasRect,
    atlas: &GlyphAtlas,
    color: crate::backend::Color,
) {
    let aw = atlas.width() as f32;
    let ah = atlas.height() as f32;
    let w = rect.w as f32;
    let h = rect.h as f32;
    let u0 = (rect.x as f32 + 0.5) / aw;
    let v0 = (rect.y as f32 + 0.5) / ah;
    let u1 = (rect.x as f32 + rect.w as f32 - 0.5) / aw;
    let v1 = (rect.y as f32 + rect.h as f32 - 0.5) / ah;
    let x0f = x0 as f32;
    let y0f = y0 as f32;
    let x1f = x0f + w;
    let y1f = y0f + h;
    let c = [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    ];
    let quad = [
        Vertex {
            pos: [x0f, y0f],
            uv: [u0, v0],
            color: c,
        },
        Vertex {
            pos: [x1f, y0f],
            uv: [u1, v0],
            color: c,
        },
        Vertex {
            pos: [x0f, y1f],
            uv: [u0, v1],
            color: c,
        },
        Vertex {
            pos: [x0f, y1f],
            uv: [u0, v1],
            color: c,
        },
        Vertex {
            pos: [x1f, y0f],
            uv: [u1, v0],
            color: c,
        },
        Vertex {
            pos: [x1f, y1f],
            uv: [u1, v1],
            color: c,
        },
    ];
    vertices.extend_from_slice(&quad);
}

/// Create the instance, adapter, and device. Headless: no surface.
fn init_device() -> Result<(wgpu::Device, wgpu::Queue), BackendError> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| BackendError::Render(format!("no GPU adapter available: {e}")))?;
        let device_desc = wgpu::DeviceDescriptor {
            label: Some("koma device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        };
        adapter
            .request_device(&device_desc)
            .await
            .map_err(|e| BackendError::Render(format!("no GPU device available: {e}")))
    })
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("koma text bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("koma text shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = create_bind_group_layout(device);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("koma text pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("koma text pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(vertex_layout)],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Round up to the next multiple of `align`.
fn align_up(n: u64, align: u64) -> u64 {
    (n + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a backend if a GPU is available; skip the assertion body
    /// otherwise (graceful software fallback per AGENTS.md).
    fn gpu() -> Option<WgpuBackend> {
        WgpuBackend::new().ok()
    }

    #[test]
    fn renders_blocks_to_frame_on_gpu() {
        let Some(mut backend) = gpu() else {
            eprintln!("skipping: no GPU adapter available");
            return;
        };
        assert!(backend.capabilities().gpu);
        let cfg = LayoutConfig {
            width: 400,
            height: 200,
            background: crate::backend::Color::rgb(0x0b, 0x0e, 0x14),
            ..Default::default()
        };
        let blocks = vec![Block {
            kind: Some(koma_core::kir::block::Kind::Paragraph(
                koma_core::kir::paragraph("The inquisitor entered the chamber."),
            )),
        }];
        let frame = backend.render_blocks(&blocks, &cfg).expect("render");
        assert_eq!(frame.width, 400);
        assert_eq!(frame.height, 200);
        // Background came through the GPU clear pass.
        assert_eq!(&frame.pixels[..4], &[0x0b, 0x0e, 0x14, 0xff]);
        // Ink present.
        let ink = frame
            .pixels
            .chunks_exact(4)
            .filter(|px| px[3] > 0 && *px != [0x0b, 0x0e, 0x14, 0xff])
            .count();
        assert!(ink > 0, "expected ink on dark background");
    }

    #[test]
    fn render_primitives_path_draws_text() {
        let Some(mut backend) = gpu() else {
            eprintln!("skipping: no GPU adapter available");
            return;
        };
        let frame = backend
            .draw(&[RenderPrimitive::Text(crate::backend::TextItem {
                text: "Hello Koma".to_owned(),
                x: 4.0,
                y: 4.0,
                font_size: 24.0,
                color: crate::backend::Color::BLACK,
                language: None,
            })])
            .expect("draw");
        let opaque = frame.pixels.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(opaque > 0, "expected ink, got {opaque}");
    }
}
