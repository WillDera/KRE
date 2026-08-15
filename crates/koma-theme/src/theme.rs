//! The `Theme` data model: a versioned, user-editable presentation spec.
//!
//! Only `typography` and `colors` take effect in the current software
//! renderer. `effects` and `particles` are declarative and versioned for
//! forward compatibility; the renderer stores and validates them but renders
//! them inert (static fallback per AGENTS.md). Audio is deferred.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Color;

/// Current theme format version. Breaking changes require migration.
pub const THEME_VERSION: &str = "0.1.0";

/// A complete theme. YAML-serializable, user-editable, shareable, versioned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    /// Theme format version (validated against [`THEME_VERSION`]).
    pub version: String,
    /// Human-readable name (e.g. "Imperial Archive").
    pub name: String,
    /// Asset attribution (AGENTS.md License and Attribution System).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<ThemeInfo>,
    #[serde(default)]
    pub typography: Typography,
    #[serde(default)]
    pub colors: ColorScheme,
    #[serde(default)]
    pub effects: HashMap<String, Effect>,
    #[serde(default)]
    pub particles: HashMap<String, Particle>,
}

impl Theme {
    /// Parse a theme from YAML bytes.
    pub fn parse_yaml(bytes: &[u8]) -> Result<Self, ThemeError> {
        serde_yaml::from_slice(bytes).map_err(ThemeError::from)
    }

    /// Serialize to YAML (stable field order via struct definition).
    pub fn to_yaml(&self) -> Result<String, ThemeError> {
        serde_yaml::to_string(self).map_err(ThemeError::from)
    }

    /// Validate format version and field sanity.
    pub fn validate(&self) -> Result<(), ThemeError> {
        if self.version != THEME_VERSION {
            return Err(ThemeError::UnsupportedVersion(
                self.version.clone(),
                THEME_VERSION,
            ));
        }
        if self.name.trim().is_empty() {
            return Err(ThemeError::Invalid("theme name must not be empty".into()));
        }
        let t = &self.typography;
        if !t.font_size.is_finite() || t.font_size <= 0.0 {
            return Err(ThemeError::Invalid(
                "typography.font_size must be > 0".into(),
            ));
        }
        if !t.line_height.is_finite() || t.line_height <= 0.0 {
            return Err(ThemeError::Invalid(
                "typography.line_height must be > 0".into(),
            ));
        }
        if !t.paragraph_spacing.is_finite() || t.paragraph_spacing < 0.0 {
            return Err(ThemeError::Invalid(
                "typography.paragraph_spacing must be >= 0".into(),
            ));
        }
        if !t.margin.is_finite() || t.margin < 0.0 {
            return Err(ThemeError::Invalid("typography.margin must be >= 0".into()));
        }
        Ok(())
    }

    /// A canonical, minimal theme for the given name (all defaults).
    pub fn minimal(name: impl Into<String>) -> Self {
        Self {
            version: THEME_VERSION.to_owned(),
            name: name.into(),
            info: None,
            typography: Typography::default(),
            colors: ColorScheme::default(),
            effects: HashMap::new(),
            particles: HashMap::new(),
        }
    }
}

/// Asset attribution (AGENTS.md: name, creator, license, source, version).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThemeInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Typography metrics consumed by the layout engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Typography {
    /// Named font family; renderer falls back to its default when absent or
    /// not installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Body font size in device pixels.
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default = "default_paragraph_spacing")]
    pub paragraph_spacing: f32,
    #[serde(default = "default_margin")]
    pub margin: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: default_font_size(),
            line_height: default_line_height(),
            paragraph_spacing: default_paragraph_spacing(),
            margin: default_margin(),
        }
    }
}

fn default_font_size() -> f32 {
    20.0
}
fn default_line_height() -> f32 {
    28.0
}
fn default_paragraph_spacing() -> f32 {
    12.0
}
fn default_margin() -> f32 {
    48.0
}

/// Named palette. Applied by the renderer (presentation truth only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColorScheme {
    #[serde(default = "default_background")]
    pub background: Color,
    #[serde(default = "default_text")]
    pub text: Color,
    #[serde(default = "default_heading")]
    pub heading: Color,
    #[serde(default = "default_quote")]
    pub quote: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            background: default_background(),
            text: default_text(),
            heading: default_heading(),
            quote: default_quote(),
        }
    }
}

fn default_background() -> Color {
    Color::WHITE
}
fn default_text() -> Color {
    Color::BLACK
}
fn default_heading() -> Color {
    Color::rgb(0x1a, 0x1a, 0x2e)
}
fn default_quote() -> Color {
    Color::rgb(0x44, 0x44, 0x44)
}

/// Declarative render effect (frost, light rays, ...). Stored and validated;
/// currently inert in the software backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Effect {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub params: HashMap<String, String>,
}

/// Declarative particle effect (snow, rain, stars, ...). Values are free-form
/// (e.g. `density: 0.5`, `speed: slow`, `wind: north`) so the schema stays
/// open while scenes mature.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Particle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("unsupported theme version `{0}` (supported: {1})")]
    UnsupportedVersion(String, &'static str),
    #[error("invalid theme: {0}")]
    Invalid(String),
    #[error("YAML parse error: {0}")]
    Yaml(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_yaml::Error> for ThemeError {
    fn from(e: serde_yaml::Error) -> Self {
        ThemeError::Yaml(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMPERIAL: &str = r##"
version: "0.1.0"
name: Imperial Archive
info:
  creator: Koma Studio
  license: MIT
typography:
  font_family: Cormorant
  font_size: 22
colors:
  background: "#0b0e14"
  text: "#e8e6e3"
effects:
  frost:
    enabled: true
    params:
      strength: "0.6"
particles:
  snow:
    density: medium
    speed: slow
    wind: north
"##;

    #[test]
    fn parses_full_theme() {
        let t = Theme::parse_yaml(IMPERIAL.as_bytes()).expect("parse");
        t.validate().expect("valid");
        assert_eq!(t.name, "Imperial Archive");
        assert_eq!(
            t.info.as_ref().unwrap().creator.as_deref(),
            Some("Koma Studio")
        );
        assert_eq!(t.typography.font_family.as_deref(), Some("Cormorant"));
        assert_eq!(t.typography.font_size, 22.0);
        assert_eq!(t.colors.background, Color::rgb(0x0b, 0x0e, 0x14));
        assert_eq!(t.colors.text, Color::rgb(0xe8, 0xe6, 0xe3));
        assert!(t.effects["frost"].enabled);
        assert_eq!(t.effects["frost"].params["strength"], "0.6");
        assert_eq!(t.particles["snow"].density.as_deref(), Some("medium"));
        assert_eq!(t.particles["snow"].wind.as_deref(), Some("north"));
    }

    #[test]
    fn defaults_fill_missing_sections() {
        let yaml = r#"
version: "0.1.0"
name: Minimal
"#;
        let t = Theme::parse_yaml(yaml.as_bytes()).expect("parse");
        t.validate().expect("valid");
        assert_eq!(t.typography.font_size, 20.0);
        assert_eq!(t.colors.background, Color::WHITE);
        assert!(t.effects.is_empty());
        assert!(t.particles.is_empty());
    }

    #[test]
    fn rejects_wrong_version() {
        let yaml = r#"
version: "9.9.9"
name: Broken
"#;
        let t = Theme::parse_yaml(yaml.as_bytes()).expect("parse");
        let err = t.validate().unwrap_err();
        assert!(matches!(err, ThemeError::UnsupportedVersion(_, _)));
    }

    #[test]
    fn round_trips_through_yaml() {
        let t = Theme::parse_yaml(IMPERIAL.as_bytes()).expect("parse");
        let out = t.to_yaml().expect("serialize");
        let t2 = Theme::parse_yaml(out.as_bytes()).expect("reparse");
        assert_eq!(t, t2);
    }
}
