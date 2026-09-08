//! Theme application: maps a [`koma_theme::Theme`] onto renderer behavior.
//!
//! Themes are presentation truth (AGENTS.md): they select typography and
//! colors but never touch content. Currently only typography and the color
//! scheme take effect; effects/particles are stored in the theme but render
//! inert in the software backend (static fallback).

use koma_theme::Theme;

use crate::backend::Color;
use crate::layout::LayoutConfig;

/// Map a theme onto a [`LayoutConfig`]. Frame size is not part of a theme
/// (it is device/user-controlled), so width/height keep defaults.
pub fn layout_config_from_theme(theme: &Theme) -> LayoutConfig {
    LayoutConfig {
        width: 800,
        height: 1000,
        margin: theme.typography.margin,
        font_size: theme.typography.font_size,
        line_height: theme.typography.line_height,
        paragraph_spacing: theme.typography.paragraph_spacing,
        text_color: theme_color(theme.colors.text),
        heading_color: theme_color(theme.colors.heading),
        quote_color: theme_color(theme.colors.quote),
        background: theme_color(theme.colors.background),
        font_family: theme.typography.font_family.clone(),
        roles: theme.resolved_roles(),
        ..Default::default()
    }
}

fn theme_color(c: koma_theme::Color) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

#[cfg(test)]
mod tests {
    use koma_theme::Theme;

    use super::*;

    fn themed() -> Theme {
        Theme::parse_yaml(
            br##"
version: "0.1.0"
name: Imperial Archive
typography:
  font_family: Cormorant
  font_size: 22
  line_height: 32
  paragraph_spacing: 14
  margin: 60
colors:
  background: "#0b0e14"
  text: "#e8e6e3"
  heading: "#cfc4ff"
  quote: "#aab4c8"
"##
            .as_slice(),
        )
        .expect("parse")
    }

    #[test]
    fn maps_typography_and_colors() {
        let cfg = layout_config_from_theme(&themed());
        assert_eq!(cfg.font_size, 22.0);
        assert_eq!(cfg.line_height, 32.0);
        assert_eq!(cfg.paragraph_spacing, 14.0);
        assert_eq!(cfg.margin, 60.0);
        assert_eq!(cfg.font_family.as_deref(), Some("Cormorant"));
        assert_eq!(cfg.background, Color::rgb(0x0b, 0x0e, 0x14));
        assert_eq!(cfg.text_color, Color::rgb(0xe8, 0xe6, 0xe3));
        assert_eq!(cfg.heading_color, Color::rgb(0xcf, 0xc4, 0xff));
        assert_eq!(cfg.quote_color, Color::rgb(0xaa, 0xb4, 0xc8));
    }

    #[test]
    fn default_theme_yields_defaults() {
        let cfg = layout_config_from_theme(&Theme::minimal("Default"));
        assert_eq!(cfg.font_size, 20.0);
        assert_eq!(cfg.margin, 48.0);
        assert_eq!(cfg.background, Color::WHITE);
        assert_eq!(cfg.font_family, None);
    }

    #[test]
    fn themed_config_renders() {
        let theme = themed();
        let cfg = layout_config_from_theme(&theme);
        let mut backend = crate::SoftwareBackend::new();
        let frame = backend
            .render_blocks(
                &[koma_core::kir::Block {
                    kind: Some(koma_core::kir::block::Kind::Paragraph(
                        koma_core::kir::paragraph("The dark chamber was silent."),
                    )),
                }],
                &cfg,
            )
            .expect("render");
        // Background is opaque dark; ink present.
        assert_eq!(&frame.pixels[..4], &[0x0b, 0x0e, 0x14, 0xff]);
        let ink = frame
            .pixels
            .chunks_exact(4)
            .filter(|p| p[3] > 0 && *p != [0x0b, 0x0e, 0x14, 0xff])
            .count();
        assert!(ink > 0, "expected ink on dark background");
    }
}
