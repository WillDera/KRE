//! The temporal engine: time-based events independent of rendering.
//!
//! Per AGENTS.md, the timeline drives page transitions, animations,
//! environmental changes, sound, particles, and typography changes. It is a
//! pure data structure — the renderer/timeline player interprets it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// What a timeline event does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    #[serde(rename = "set_background")]
    SetBackground,
    #[serde(rename = "set_lighting")]
    SetLighting,
    #[serde(rename = "set_frost")]
    SetFrost,
    #[serde(rename = "reveal")]
    Reveal,
    #[serde(rename = "fade_in")]
    FadeIn,
    #[serde(rename = "fade_out")]
    FadeOut,
    #[serde(rename = "start_effect")]
    StartEffect,
    #[serde(rename = "stop_effect")]
    StopEffect,
    /// Ambient audio trigger (deferred; engine must function without audio).
    #[serde(rename = "play_ambient")]
    PlayAmbient,
}

/// A timeline action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineAction {
    pub kind: ActionKind,
    /// Target node/effect id; `None` means the whole scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Numeric parameters (opacity, intensity, ...).
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

impl Default for TimelineAction {
    fn default() -> Self {
        Self {
            kind: ActionKind::Reveal,
            target: None,
            params: HashMap::new(),
        }
    }
}

/// One timed event: "At 0.0s: fade background".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEvent {
    /// Seconds from timeline start.
    pub time: f32,
    pub action: TimelineAction,
}

impl Default for TimelineEvent {
    fn default() -> Self {
        Self {
            time: 0.0,
            action: TimelineAction::default(),
        }
    }
}

/// A chapter's timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timeline {
    /// Total duration in seconds.
    pub duration: f32,
    #[serde(default)]
    pub events: Vec<TimelineEvent>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            duration: 0.0,
            events: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_round_trips() {
        let t = Timeline {
            duration: 5.0,
            events: vec![
                TimelineEvent {
                    time: 0.0,
                    action: TimelineAction {
                        kind: ActionKind::SetBackground,
                        target: Some("root".to_owned()),
                        params: HashMap::new(),
                    },
                },
                TimelineEvent {
                    time: 2.0,
                    action: TimelineAction {
                        kind: ActionKind::SetFrost,
                        target: None,
                        params: HashMap::from([("intensity".to_owned(), 0.5)]),
                    },
                },
            ],
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Timeline = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        assert_eq!(back.events[1].time, 2.0);
    }
}
