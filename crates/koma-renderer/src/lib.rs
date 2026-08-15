//! koma-renderer: rendering primitives, backend abstraction, text rendering.
//!
//! Phase 4 scope (per AGENTS.md development phases):
//! - **backend abstraction** — [`RenderBackend`] defines rendering primitives
//!   (text, images, particles, shaders, animations, transforms). The scene
//!   system must not depend on any concrete backend (notably wgpu).
//! - **layout** — independent of rendering; shapes text with cosmic-text.
//! - **software backend** — CPU rasterizer (swash) for text/images; the
//!   fallback and testable path. wgpu is the Phase 7 GPU backend target.
//!
//! Backend options per AGENTS.md: wgpu / Vulkan / Metal / WebGPU / software.

pub mod backend;
pub mod layout;
pub mod scene;
pub mod software;
pub mod theme;

pub use backend::{
    AnimationItem, BackendError, Capabilities, Color, Frame, ImageItem, ParticleItem,
    RenderBackend, RenderPrimitive, ShaderItem, TextItem, TransformItem,
};
pub use layout::{LayoutConfig, PlacedGlyph, PlacedLine, layout_blocks};
pub use scene::layout_config_from_scene;
pub use software::SoftwareBackend;
pub use theme::layout_config_from_theme;
