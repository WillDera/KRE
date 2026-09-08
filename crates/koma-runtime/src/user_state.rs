//! User state stored separately from `.koma` packages.
//!
//! Highlights, bookmarks, and reading position must survive book updates
//! (AGENTS.md User State System / Immutability Model).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;
use koma_renderer::{MarkKind, TextAnchor, TextRange};

/// Persistent reading position.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadingPosition {
    pub chapter_id: String,
    /// 0-based page within the chapter.
    #[serde(default)]
    pub page: usize,
}

/// A bookmark in user state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bookmark {
    pub id: String,
    pub chapter_id: String,
    #[serde(default)]
    pub page: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A highlight / note stored as content-layer ranges (never raster).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredMark {
    pub id: String,
    pub chapter_id: String,
    pub kind: MarkKindSerde,
    pub range: TextRangeSerde,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarkKindSerde {
    Highlight,
    Note,
    Bookmark,
}

impl From<MarkKind> for MarkKindSerde {
    fn from(k: MarkKind) -> Self {
        match k {
            MarkKind::Highlight => Self::Highlight,
            MarkKind::Note => Self::Note,
            MarkKind::Bookmark => Self::Bookmark,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextRangeSerde {
    pub start_block: usize,
    pub start_offset: u32,
    pub end_block: usize,
    pub end_offset: u32,
}

impl From<TextRange> for TextRangeSerde {
    fn from(r: TextRange) -> Self {
        Self {
            start_block: r.start.block,
            start_offset: r.start.offset,
            end_block: r.end.block,
            end_offset: r.end.offset,
        }
    }
}

impl TextRangeSerde {
    pub fn to_range(self) -> TextRange {
        TextRange::new(
            TextAnchor::new(self.start_block, self.start_offset),
            TextAnchor::new(self.end_block, self.end_offset),
        )
    }
}

/// Full user state document (JSON on disk).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserState {
    pub version: String,
    /// Stable key tying this state to a package (seed + title hash, etc.).
    pub package_key: String,
    pub position: ReadingPosition,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default)]
    pub marks: Vec<StoredMark>,
}

impl UserState {
    pub const VERSION: &'static str = "0.1.0";

    pub fn new(package_key: impl Into<String>, chapter_id: impl Into<String>) -> Self {
        let chapter_id = chapter_id.into();
        Self {
            version: Self::VERSION.to_owned(),
            package_key: package_key.into(),
            position: ReadingPosition {
                chapter_id,
                page: 0,
            },
            bookmarks: Vec::new(),
            marks: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, RuntimeError> {
        let bytes = fs::read(path)?;
        let state: Self = serde_json::from_slice(&bytes)
            .map_err(|e| RuntimeError::UserState(e.to_string()))?;
        if state.version != Self::VERSION {
            return Err(RuntimeError::UserState(format!(
                "unsupported user-state version `{}`",
                state.version
            )));
        }
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<(), RuntimeError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| RuntimeError::UserState(e.to_string()))?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Default path: `{dir}/{package_key}.user.json`.
    pub fn default_path(dir: &Path, package_key: &str) -> PathBuf {
        dir.join(format!("{package_key}.user.json"))
    }
}
