//! Scene application: maps a [`koma_scene::Scene`] onto renderer behavior.
//!
//! Scenes are presentation truth. The scene adapter overrides the renderer's
//! base configuration with the scene's environment background and text-layer
//! typography. Effects, lighting, and the timeline are carried by the scene
//! but currently render inert in the software backend (static fallback per
//! AGENTS.md failure handling).

use koma_scene::{NodeKind, Scene};

use crate::backend::Color;
use crate::layout::LayoutConfig;

/// Apply a scene on top of a base [`LayoutConfig`]: environment background and
/// the text layer's typography win over the base values.
pub fn layout_config_from_scene(scene: &Scene, base: &LayoutConfig) -> LayoutConfig {
    let mut cfg = base.clone();
    cfg.background = scene_color(scene.environment.background);
    let text_layer = scene
        .root
        .children
        .iter()
        .find(|n| n.kind == NodeKind::TextLayer)
        .and_then(|n| n.typography.as_ref());
    if let Some(typography) = text_layer {
        cfg.margin = typography.margin;
        cfg.font_size = typography.font_size;
        cfg.line_height = typography.line_height;
        cfg.paragraph_spacing = typography.paragraph_spacing;
        cfg.font_family = typography.font_family.clone();
    }
    cfg
}

fn scene_color(c: koma_theme::Color) -> Color {
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

    #[test]
    fn scene_overrides_background_and_text_typography() {
        let theme = Theme::minimal("Default");
        let scene = koma_scene::default_scene_for_chapter("ch1", &theme, &[]);
        scene.validate().expect("valid scene");

        let base = LayoutConfig {
            width: 800,
            height: 1000,
            ..LayoutConfig::default()
        };
        let cfg = layout_config_from_scene(&scene, &base);
        assert_eq!(cfg.background, Color::WHITE);
        assert_eq!(cfg.font_size, 20.0);
        assert_eq!(cfg.font_family, None);

        // Still renders.
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
        let ink = frame
            .pixels
            .chunks_exact(4)
            .filter(|p| p[3] > 0 && *p != [0xff, 0xff, 0xff, 0xff])
            .count();
        assert!(ink > 0, "expected ink on scene background");
    }
}
