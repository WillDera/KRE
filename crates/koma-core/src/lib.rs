//! Koma Rendering Engine core.
//!
//! Responsibilities (per AGENTS.md):
//! - shared types
//! - KIR structures
//! - serialization
//! - errors
//! - plugin interfaces
//!
//! Koma is a general-purpose narrative rendering platform, not an ebook
//! reader. This crate is source-format agnostic: it defines KIR and the
//! adapter/plugin interfaces. The renderer consumes only KIR and never
//! parses source formats.

pub mod adapters;
pub mod error;
pub mod kir;
pub mod plugin;

pub use adapters::{AdapterError, ContentAdapter, ContentSource};
pub use error::{EncodeError, KirError};
pub use plugin::{PermissionSet, Plugin, PluginId, PluginKind, PluginManifest};
