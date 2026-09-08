//! Genre presets and per-role typography (theme format v0.2).

use serde::{Deserialize, Serialize};

/// Font weight for a role.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

/// Drop-cap presentation for body text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DropCap {
    #[serde(default)]
    pub enabled: bool,
    /// How many body lines the drop cap spans (clamped to >= 1).
    #[serde(default = "default_drop_lines")]
    pub lines: u32,
    /// Size multiplier relative to body font size.
    #[serde(default = "default_drop_scale")]
    pub scale: f32,
}

impl Default for DropCap {
    fn default() -> Self {
        Self {
            enabled: false,
            lines: default_drop_lines(),
            scale: default_drop_scale(),
        }
    }
}

fn default_drop_lines() -> u32 {
    3
}
fn default_drop_scale() -> f32 {
    3.0
}

/// Decorative rule drawn under a heading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecorativeRule {
    #[serde(default)]
    pub enabled: bool,
    /// Rule thickness in device pixels.
    #[serde(default = "default_rule_thickness")]
    pub thickness: f32,
    /// Gap between the last heading line and the rule.
    #[serde(default = "default_rule_gap")]
    pub gap: f32,
}

impl Default for DecorativeRule {
    fn default() -> Self {
        Self {
            enabled: false,
            thickness: default_rule_thickness(),
            gap: default_rule_gap(),
        }
    }
}

fn default_rule_thickness() -> f32 {
    1.0
}
fn default_rule_gap() -> f32 {
    8.0
}

/// Per-role presentation style (body, headings, quote).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RoleStyle {
    /// Indent applied to the first line of the block (device px).
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub first_line_indent: f32,
    /// Justify wrapped lines when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub justify: bool,
    #[serde(default, skip_serializing_if = "is_normal_weight")]
    pub weight: FontWeight,
    /// Heading size multiplier relative to body `font_size`. Ignored for body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    #[serde(default, skip_serializing_if = "DropCap::is_disabled")]
    pub drop_cap: DropCap,
    #[serde(default, skip_serializing_if = "DecorativeRule::is_disabled")]
    pub decorative_rule: DecorativeRule,
}

impl DropCap {
    fn is_disabled(&self) -> bool {
        !self.enabled
    }
}

impl DecorativeRule {
    fn is_disabled(&self) -> bool {
        !self.enabled
    }
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

fn is_normal_weight(w: &FontWeight) -> bool {
    matches!(w, FontWeight::Normal)
}

/// Resolved role styles used by the layout engine (deterministic, no maps).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoleStyleSet {
    pub body: RoleStyle,
    pub h1: RoleStyle,
    pub h2: RoleStyle,
    pub h3: RoleStyle,
    pub quote: RoleStyle,
}

impl RoleStyleSet {
    /// Merge explicit theme roles over a genre preset (theme wins on set fields
    /// that differ from default — we overlay non-default theme values).
    pub fn merge(preset: RoleStyleSet, overlay: &std::collections::HashMap<String, RoleStyle>) -> Self {
        let mut out = preset;
        if let Some(r) = overlay.get("body") {
            out.body = merge_role(&out.body, r);
        }
        if let Some(r) = overlay.get("h1").or_else(|| overlay.get("heading")) {
            out.h1 = merge_role(&out.h1, r);
        }
        if let Some(r) = overlay.get("h2") {
            out.h2 = merge_role(&out.h2, r);
        }
        if let Some(r) = overlay.get("h3") {
            out.h3 = merge_role(&out.h3, r);
        }
        if let Some(r) = overlay.get("quote") {
            out.quote = merge_role(&out.quote, r);
        }
        out
    }
}

fn merge_role(base: &RoleStyle, over: &RoleStyle) -> RoleStyle {
    RoleStyle {
        first_line_indent: if over.first_line_indent != 0.0 {
            over.first_line_indent
        } else {
            base.first_line_indent
        },
        justify: over.justify || base.justify,
        weight: if !matches!(over.weight, FontWeight::Normal) {
            over.weight
        } else {
            base.weight
        },
        scale: over.scale.or(base.scale),
        drop_cap: if over.drop_cap.enabled {
            over.drop_cap.clone()
        } else {
            base.drop_cap.clone()
        },
        decorative_rule: if over.decorative_rule.enabled {
            over.decorative_rule.clone()
        } else {
            base.decorative_rule.clone()
        },
    }
}

/// Built-in genre presets. Unknown genres yield defaults (no-op styles).
pub fn genre_preset(genre: &str) -> RoleStyleSet {
    match genre.to_ascii_lowercase().as_str() {
        "literary" | "literature" => RoleStyleSet {
            body: RoleStyle {
                first_line_indent: 28.0,
                justify: true,
                drop_cap: DropCap {
                    enabled: true,
                    lines: 3,
                    scale: 3.0,
                },
                ..Default::default()
            },
            h1: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.6),
                decorative_rule: DecorativeRule {
                    enabled: true,
                    thickness: 1.0,
                    gap: 10.0,
                },
                ..Default::default()
            },
            h2: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.35),
                ..Default::default()
            },
            h3: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.2),
                ..Default::default()
            },
            quote: RoleStyle {
                first_line_indent: 16.0,
                ..Default::default()
            },
        },
        "fantasy" => RoleStyleSet {
            body: RoleStyle {
                first_line_indent: 24.0,
                drop_cap: DropCap {
                    enabled: true,
                    lines: 3,
                    scale: 3.2,
                },
                ..Default::default()
            },
            h1: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.7),
                decorative_rule: DecorativeRule {
                    enabled: true,
                    thickness: 1.5,
                    gap: 12.0,
                },
                ..Default::default()
            },
            h2: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.4),
                ..Default::default()
            },
            h3: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.2),
                ..Default::default()
            },
            quote: RoleStyle::default(),
        },
        "technical" | "textbook" => RoleStyleSet {
            body: RoleStyle {
                first_line_indent: 0.0,
                justify: false,
                ..Default::default()
            },
            h1: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.5),
                ..Default::default()
            },
            h2: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.3),
                ..Default::default()
            },
            h3: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.15),
                ..Default::default()
            },
            quote: RoleStyle::default(),
        },
        "noir" => RoleStyleSet {
            body: RoleStyle {
                first_line_indent: 20.0,
                justify: true,
                ..Default::default()
            },
            h1: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.45),
                decorative_rule: DecorativeRule {
                    enabled: true,
                    thickness: 2.0,
                    gap: 8.0,
                },
                ..Default::default()
            },
            h2: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.25),
                ..Default::default()
            },
            h3: RoleStyle {
                weight: FontWeight::Bold,
                scale: Some(1.1),
                ..Default::default()
            },
            quote: RoleStyle::default(),
        },
        _ => RoleStyleSet::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literary_preset_enables_drop_cap_and_indent() {
        let p = genre_preset("literary");
        assert!(p.body.drop_cap.enabled);
        assert!(p.body.first_line_indent > 0.0);
        assert!(p.h1.decorative_rule.enabled);
        assert_eq!(p.h1.weight, FontWeight::Bold);
    }

    #[test]
    fn unknown_genre_is_noop() {
        let p = genre_preset("unknown-xyz");
        assert_eq!(p, RoleStyleSet::default());
    }
}
