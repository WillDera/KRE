//! koma-theme: the theme engine's versioned data format.
//!
//! Themes are **presentation truth** (per AGENTS.md): they never alter content
//! or semantics. They are serializable (YAML), user-editable, shareable, and
//! versioned. A theme is a standalone artifact: it can be embedded in a `.koma`
//! package by the compiler or passed to the renderer directly.
//!
//! This crate is a pure data model. It does not depend on the renderer or the
//! compiler; both consume [`Theme`] and map it onto their own behavior.

pub mod color;
pub mod genre;
pub mod theme;

pub use color::Color;
pub use genre::{
    DecorativeRule, DropCap, FontWeight, RoleStyle, RoleStyleSet, genre_preset,
};
pub use theme::{
    ColorScheme, Effect, Particle, SUPPORTED_THEME_VERSIONS, THEME_VERSION, Theme, ThemeError,
    ThemeInfo, Typography,
};
