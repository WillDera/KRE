//! Lazy chapter window: current chapter plus nearby neighbors.

/// The set of chapter ids the runtime should keep resident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterWindow {
    pub current: String,
    /// Neighbor ids in spine order (includes `current`).
    pub loaded: Vec<String>,
}

impl ChapterWindow {
    /// Build a window of `radius` chapters on each side of `current` within
    /// the spine `chapter_ids`.
    pub fn around(chapter_ids: &[String], current: &str, radius: usize) -> Option<Self> {
        let idx = chapter_ids.iter().position(|id| id == current)?;
        let start = idx.saturating_sub(radius);
        let end = (idx + radius + 1).min(chapter_ids.len());
        Some(Self {
            current: current.to_owned(),
            loaded: chapter_ids[start..end].to_vec(),
        })
    }

    pub fn contains(&self, id: &str) -> bool {
        self.loaded.iter().any(|c| c == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clamps_at_edges() {
        let ids: Vec<String> = (0..5).map(|i| format!("ch{i}")).collect();
        let w = ChapterWindow::around(&ids, "ch0", 1).unwrap();
        assert_eq!(
            w.loaded,
            vec!["ch0".to_string(), "ch1".to_string()]
        );
        let w = ChapterWindow::around(&ids, "ch4", 1).unwrap();
        assert_eq!(
            w.loaded,
            vec!["ch3".to_string(), "ch4".to_string()]
        );
        let w = ChapterWindow::around(&ids, "ch2", 1).unwrap();
        assert_eq!(
            w.loaded,
            vec!["ch1".to_string(), "ch2".to_string(), "ch3".to_string()]
        );
    }
}
