//! koma-scene: the scene graph — environments, lighting, effects, typography,
//! transitions, and timelines.
//!
//! Scenes are **presentation truth** (AGENTS.md): they describe how content is
//! displayed, never the content itself. They are versioned, serializable
//! (JSON), generated deterministically at compile time, and stored per chapter
//! in `.koma` packages for lazy loading.
//!
//! The scene system is rendering-agnostic: it must not depend on any concrete
//! backend (notably wgpu). The renderer maps scenes onto its primitives.

pub mod effects;
pub mod scene;
pub mod synthesis;
pub mod timeline;

pub use effects::{Effect, EffectKind};
pub use scene::{
    DirectionalLight, Environment, EnvironmentKind, LightSource, Lighting, Node, NodeKind,
    SCENE_VERSION, Scene, SceneError, Transition, TransitionKind,
};
pub use synthesis::default_scene_for_chapter;
pub use timeline::{ActionKind, Timeline, TimelineAction, TimelineEvent};
