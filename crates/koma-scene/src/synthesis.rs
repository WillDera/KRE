//! Deterministic default scene synthesis.
//!
//! Scene generation is a compiler responsibility (AGENTS.md). Given a chapter
//! id, a theme, and the chapter's blocks, this produces a deterministic
//! default scene: environment background from the theme palette, ambient
//! lighting, a text layer carrying the theme typography, effect layers for
//! enabled theme effects/particles, an opening fade-in timeline, and an entry
//! transition.
//!
//! Determinism: hash-map iteration is randomized, so collection keys are
//! sorted before iteration. Identical inputs produce identical scenes.

use koma_core::kir::Block;
use koma_theme::Theme;

use crate::effects::{Effect, EffectKind};
use crate::scene::{
    Environment, EnvironmentKind, LightSource, Lighting, Node, NodeKind, Scene, Transition,
    TransitionKind,
};
use crate::timeline::{ActionKind, Timeline, TimelineAction, TimelineEvent};

/// Chapter-level semantic hints used to refine presentation (mood /
/// environment). Owned by analysis; never content truth.
#[derive(Debug, Clone, Default)]
pub struct SceneAnalysisHints {
    pub mood: Option<String>,
    pub environment: Option<String>,
}

/// Build the default scene for one chapter.
#[allow(clippy::field_reassign_with_default)]
pub fn default_scene_for_chapter(chapter_id: &str, theme: &Theme, blocks: &[Block]) -> Scene {
    default_scene_for_chapter_with_hints(chapter_id, theme, blocks, None)
}

/// Build the default scene, optionally applying compile-time analysis hints.
pub fn default_scene_for_chapter_with_hints(
    chapter_id: &str,
    theme: &Theme,
    _blocks: &[Block],
    hints: Option<&SceneAnalysisHints>,
) -> Scene {
    let mut scene = Scene::default();
    scene.chapter_id = chapter_id.to_owned();

    // Environment: presentation truth from the theme palette.
    scene.environment = Environment {
        kind: EnvironmentKind::Abstract,
        background: theme.colors.background,
        atmosphere: Default::default(),
    };
    scene.lighting = Lighting {
        ambient: LightSource {
            color: theme.colors.text,
            intensity: 0.85,
        },
        directional: None,
    };

    // Scene graph: text layer (theme typography) + effect layers.
    let text_layer = Node {
        id: "text".to_owned(),
        kind: NodeKind::TextLayer,
        typography: Some(theme.typography.clone()),
        animation: Some("open".to_owned()),
        ..Node::default()
    };

    let mut effects = Vec::new();
    let mut effect_keys: Vec<&String> = theme.effects.keys().collect();
    effect_keys.sort();
    for id in effect_keys {
        let e = &theme.effects[id];
        if e.enabled {
            effects.push(Effect {
                id: format!("effect-{id}"),
                kind: EffectKind::Particles,
                enabled: true,
                params: e.params.clone(),
            });
        }
    }
    let mut particle_keys: Vec<&String> = theme.particles.keys().collect();
    particle_keys.sort();
    for id in particle_keys {
        let p = &theme.particles[id];
        let mut params = p.params.clone();
        if let Some(d) = &p.density {
            params.insert("density".to_owned(), d.clone());
        }
        if let Some(s) = &p.speed {
            params.insert("speed".to_owned(), s.clone());
        }
        if let Some(w) = &p.wind {
            params.insert("wind".to_owned(), w.clone());
        }
        effects.push(Effect {
            id: format!("particle-{id}"),
            kind: effect_kind_for(id),
            enabled: true,
            params,
        });
    }

    let mut children = vec![text_layer];
    if !effects.is_empty() {
        children.push(Node {
            id: "effects".to_owned(),
            kind: NodeKind::EffectLayer,
            effects,
            ..Node::default()
        });
    }
    scene.root = Node {
        id: "root".to_owned(),
        kind: NodeKind::Root,
        children,
        ..Node::default()
    };

    // Opening timeline: background then fade in the text layer.
    scene.timeline = Timeline {
        duration: 1.2,
        events: vec![
            TimelineEvent {
                time: 0.0,
                action: TimelineAction {
                    kind: ActionKind::SetBackground,
                    target: Some("root".to_owned()),
                    params: Default::default(),
                },
            },
            TimelineEvent {
                time: 0.5,
                action: TimelineAction {
                    kind: ActionKind::FadeIn,
                    target: Some("text".to_owned()),
                    params: Default::default(),
                },
            },
        ],
    };

    // Entry transition into this chapter.
    scene.transitions = vec![Transition {
        from: None,
        to: chapter_id.to_owned(),
        kind: TransitionKind::Fade,
        duration: 0.6,
    }];

    if let Some(hints) = hints {
        apply_chapter_analysis(&mut scene, hints);
    }

    scene
}

/// Refine a scene from chapter analysis. Presentation only — never touches
/// content. Deterministic for identical hints.
pub fn apply_chapter_analysis(scene: &mut Scene, hints: &SceneAnalysisHints) {
    if let Some(env) = hints.environment.as_deref() {
        scene.environment.kind = environment_kind(env);
        if env.eq_ignore_ascii_case("frozen") {
            scene.environment.atmosphere.insert("frost".to_owned(), 0.6);
        }
    }
    if let Some(mood) = hints.mood.as_deref() {
        match mood.to_ascii_lowercase().as_str() {
            "ominous" => {
                scene
                    .environment
                    .atmosphere
                    .entry("frost".to_owned())
                    .or_insert(0.45);
            }
            "dark" => {
                scene.lighting.ambient.intensity = scene.lighting.ambient.intensity.min(0.55);
            }
            "warm" => {
                scene.lighting.ambient.intensity = scene.lighting.ambient.intensity.max(0.95);
            }
            _ => {}
        }
    }
}

fn environment_kind(label: &str) -> EnvironmentKind {
    match label.to_ascii_lowercase().as_str() {
        "space" => EnvironmentKind::Space,
        "indoor" => EnvironmentKind::Indoor,
        "outdoor" | "frozen" => EnvironmentKind::Outdoor,
        _ => EnvironmentKind::Abstract,
    }
}

/// Map a theme particle name onto an effect kind.
fn effect_kind_for(id: &str) -> EffectKind {
    match id.to_ascii_lowercase().as_str() {
        "snow" => EffectKind::Snow,
        "rain" => EffectKind::Rain,
        "fog" => EffectKind::Fog,
        "dust" => EffectKind::Dust,
        "stars" => EffectKind::Stars,
        "lightrays" | "light_rays" => EffectKind::LightRays,
        _ => EffectKind::Particles,
    }
}

#[cfg(test)]
mod tests {
    use koma_theme::{Color, Theme};

    use super::*;

    fn themed() -> Theme {
        Theme::parse_yaml(
            br##"
version: "0.1.0"
name: Imperial Archive
typography:
  font_family: Cormorant
  font_size: 22
colors:
  background: "#0b0e14"
  text: "#e8e6e3"
effects:
  frost:
    enabled: true
particles:
  snow:
    density: medium
    speed: slow
"##
            .as_slice(),
        )
        .expect("parse")
    }

    #[test]
    fn synthesis_applies_theme_and_structure() {
        let theme = themed();
        let scene = default_scene_for_chapter("ch1", &theme, &[]);
        scene.validate().expect("valid scene");
        assert_eq!(scene.chapter_id, "ch1");
        assert_eq!(scene.environment.background, Color::rgb(0x0b, 0x0e, 0x14));
        assert_eq!(scene.lighting.ambient.color, Color::rgb(0xe8, 0xe6, 0xe3));

        let text = scene
            .root
            .children
            .iter()
            .find(|n| n.kind == NodeKind::TextLayer)
            .expect("text layer");
        assert_eq!(
            text.typography.as_ref().unwrap().font_family.as_deref(),
            Some("Cormorant")
        );
        assert_eq!(text.animation.as_deref(), Some("open"));

        let effects = scene
            .root
            .children
            .iter()
            .find(|n| n.kind == NodeKind::EffectLayer)
            .expect("effect layer");
        assert_eq!(effects.effects.len(), 2);
        assert_eq!(effects.effects[0].id, "effect-frost");
        assert_eq!(effects.effects[1].kind, EffectKind::Snow);
        assert_eq!(effects.effects[1].params["density"], "medium");

        assert_eq!(scene.timeline.events.len(), 2);
        assert_eq!(scene.timeline.events[1].time, 0.5);
        assert_eq!(scene.transitions.len(), 1);
        assert_eq!(scene.transitions[0].kind, TransitionKind::Fade);
    }

    #[test]
    fn synthesis_is_deterministic() {
        let theme = themed();
        let a = default_scene_for_chapter("ch1", &theme, &[]);
        let b = default_scene_for_chapter("ch1", &theme, &[]);
        assert_eq!(a.to_json().unwrap(), b.to_json().unwrap());
    }

    #[test]
    fn unknown_particle_maps_to_generic() {
        let theme = Theme {
            particles: std::collections::HashMap::from([(
                "glow".to_owned(),
                koma_theme::Particle {
                    density: Some("high".into()),
                    ..Default::default()
                },
            )]),
            ..themed()
        };
        let scene = default_scene_for_chapter("ch1", &theme, &[]);
        let layer = scene
            .root
            .children
            .iter()
            .find(|n| n.kind == NodeKind::EffectLayer)
            .expect("effect layer");
        assert_eq!(layer.effects[0].kind, EffectKind::Particles);
    }

    #[test]
    fn analysis_hints_set_environment_and_atmosphere() {
        let theme = themed();
        let hints = SceneAnalysisHints {
            mood: Some("ominous".into()),
            environment: Some("space".into()),
        };
        let scene = default_scene_for_chapter_with_hints("ch1", &theme, &[], Some(&hints));
        assert_eq!(scene.environment.kind, EnvironmentKind::Space);
        assert!(scene.environment.atmosphere.contains_key("frost"));
    }
}
