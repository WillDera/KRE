//! Declarative procedural effects (snow, rain, fog, ...).
//!
//! Per AGENTS.md the procedural effect system is declarative; the runtime
//! never computes these itself. Effects are versioned here and applied by the
//! renderer (software backend currently renders them inert — static fallback).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The kind of procedural effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectKind {
    #[serde(rename = "snow")]
    Snow,
    #[serde(rename = "rain")]
    Rain,
    #[serde(rename = "fog")]
    Fog,
    #[serde(rename = "dust")]
    Dust,
    #[serde(rename = "stars")]
    Stars,
    #[serde(rename = "light_rays")]
    LightRays,
    #[serde(rename = "particles")]
    Particles,
}

/// A declarative effect attached to a scene node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Effect {
    pub id: String,
    pub kind: EffectKind,
    #[serde(default)]
    pub enabled: bool,
    /// Free-form parameters (`density`, `speed`, `wind`, ...).
    #[serde(default)]
    pub params: HashMap<String, String>,
}

impl Default for Effect {
    fn default() -> Self {
        Self {
            id: "effect".to_owned(),
            kind: EffectKind::Particles,
            enabled: true,
            params: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_round_trips() {
        let e = Effect {
            id: "snow-1".to_owned(),
            kind: EffectKind::Snow,
            enabled: true,
            params: HashMap::from([
                ("density".to_owned(), "0.5".to_owned()),
                ("wind".to_owned(), "north".to_owned()),
            ]),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
