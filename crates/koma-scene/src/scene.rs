//! The `Scene` root: environment, lighting, scene graph, transitions.

use std::collections::HashMap;

use koma_theme::Color;
use serde::{Deserialize, Serialize};

use crate::effects::Effect;
use crate::timeline::Timeline;

/// Current scene format version. Breaking changes require migration.
pub const SCENE_VERSION: &str = "0.1.0";

/// The kind of environment a scene depicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentKind {
    #[serde(rename = "indoor")]
    Indoor,
    #[serde(rename = "outdoor")]
    Outdoor,
    #[serde(rename = "space")]
    Space,
    #[serde(rename = "abstract")]
    Abstract,
}

/// Atmosphere / environment parameters of a scene.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Environment {
    pub kind: EnvironmentKind,
    /// Base background color.
    pub background: Color,
    /// Declarative atmosphere parameters (e.g. `frost`, `haze`).
    #[serde(default)]
    pub atmosphere: HashMap<String, f32>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            kind: EnvironmentKind::Abstract,
            background: Color::WHITE,
            atmosphere: HashMap::new(),
        }
    }
}

/// A directional light (sun / moon / key light).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DirectionalLight {
    pub color: Color,
    pub intensity: f32,
    /// Normalized direction vector (x, y, z).
    pub direction: [f32; 3],
}

/// An ambient light source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LightSource {
    pub color: Color,
    /// 0.0 = dark, 1.0 = full.
    pub intensity: f32,
}

impl Default for LightSource {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 0.8,
        }
    }
}

/// Lighting configuration for a scene.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct Lighting {
    pub ambient: LightSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directional: Option<DirectionalLight>,
}

/// What a scene-graph node represents in the visual composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    #[serde(rename = "root")]
    Root,
    #[serde(rename = "background")]
    BackgroundLayer,
    #[serde(rename = "text")]
    TextLayer,
    #[serde(rename = "effects")]
    EffectLayer,
    #[serde(rename = "image")]
    ImageLayer,
}

/// A node in the scene graph. Children compose under their parent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub children: Vec<Node>,
    /// Effects attached to this layer.
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Typography override for this layer (presentation only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typography: Option<koma_theme::Typography>,
    /// Timeline animation this node participates in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<String>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            id: "root".to_owned(),
            kind: NodeKind::Root,
            children: Vec::new(),
            effects: Vec::new(),
            typography: None,
            animation: None,
        }
    }
}

/// How one scene gives way to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    #[serde(rename = "fade")]
    Fade,
    #[serde(rename = "slide")]
    Slide,
    #[serde(rename = "none")]
    None,
}

/// A transition between scenes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transition {
    /// `None` when this is the entry transition into the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
    pub kind: TransitionKind,
    /// Duration in seconds.
    pub duration: f32,
}

/// A complete scene for one chapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scene {
    pub version: String,
    /// Chapter this scene presents.
    pub chapter_id: String,
    pub environment: Environment,
    pub lighting: Lighting,
    pub root: Node,
    #[serde(default)]
    pub transitions: Vec<Transition>,
    pub timeline: Timeline,
}

impl Scene {
    /// Parse a scene from JSON bytes.
    pub fn parse_json(bytes: &[u8]) -> Result<Self, SceneError> {
        serde_json::from_slice(bytes).map_err(SceneError::from)
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, SceneError> {
        serde_json::to_string(self).map_err(SceneError::from)
    }

    /// Validate format version and field sanity.
    pub fn validate(&self) -> Result<(), SceneError> {
        if self.version != SCENE_VERSION {
            return Err(SceneError::UnsupportedVersion(
                self.version.clone(),
                SCENE_VERSION,
            ));
        }
        if self.chapter_id.is_empty() {
            return Err(SceneError::Invalid(
                "scene chapter_id must not be empty".into(),
            ));
        }
        let ambient = &self.lighting.ambient;
        if !ambient.intensity.is_finite() || ambient.intensity < 0.0 {
            return Err(SceneError::Invalid(
                "lighting.ambient.intensity must be >= 0".into(),
            ));
        }
        if let Some(d) = &self.lighting.directional {
            if !d.intensity.is_finite() || d.intensity < 0.0 {
                return Err(SceneError::Invalid(
                    "lighting.directional.intensity must be >= 0".into(),
                ));
            }
        }
        if !self.timeline.duration.is_finite() || self.timeline.duration < 0.0 {
            return Err(SceneError::Invalid("timeline.duration must be >= 0".into()));
        }
        for event in &self.timeline.events {
            if !event.time.is_finite() || event.time < 0.0 {
                return Err(SceneError::Invalid(
                    "timeline event time must be >= 0".into(),
                ));
            }
        }
        for t in &self.transitions {
            if !t.duration.is_finite() || t.duration < 0.0 {
                return Err(SceneError::Invalid(
                    "transition duration must be >= 0".into(),
                ));
            }
        }
        Ok(())
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            version: SCENE_VERSION.to_owned(),
            chapter_id: String::new(),
            environment: Environment::default(),
            lighting: Lighting::default(),
            root: Node::default(),
            transitions: Vec::new(),
            timeline: Timeline::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("unsupported scene version `{0}` (supported: {1})")]
    UnsupportedVersion(String, &'static str),
    #[error("invalid scene: {0}")]
    Invalid(String),
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for SceneError {
    fn from(e: serde_json::Error) -> Self {
        SceneError::Json(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scene_is_valid() {
        let scene = Scene {
            chapter_id: "ch1".to_owned(),
            ..Scene::default()
        };
        scene.validate().expect("default scene valid");
    }

    #[test]
    fn round_trips_through_json() {
        let scene = Scene {
            version: SCENE_VERSION.to_owned(),
            chapter_id: "ch1".to_owned(),
            environment: Environment {
                kind: EnvironmentKind::Outdoor,
                background: Color::rgb(0x0b, 0x0e, 0x14),
                atmosphere: HashMap::from([("frost".to_owned(), 0.6)]),
            },
            lighting: Lighting {
                ambient: LightSource {
                    color: Color::rgb(0xe8, 0xe6, 0xe3),
                    intensity: 0.7,
                },
                directional: Some(DirectionalLight {
                    color: Color::WHITE,
                    intensity: 0.4,
                    direction: [0.0, -1.0, 0.5],
                }),
            },
            ..Scene::default()
        };
        let json = scene.to_json().expect("serialize");
        let back = Scene::parse_json(json.as_bytes()).expect("parse");
        back.validate().expect("valid");
        assert_eq!(back, scene);
    }

    #[test]
    fn rejects_wrong_version() {
        let scene = Scene {
            version: "9.9.9".to_owned(),
            ..Scene::default()
        };
        let err = scene.validate().unwrap_err();
        assert!(matches!(err, SceneError::UnsupportedVersion(_, _)));
    }
}
