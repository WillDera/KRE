//! Reading session over an opened `.koma` package.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use koma_compiler::KomaPackage;
use koma_core::kir::Chapter;
use koma_scene::Scene;

use crate::error::RuntimeError;
use crate::user_state::UserState;
use crate::window::ChapterWindow;

/// Default neighbor radius for the lazy chapter window.
pub const DEFAULT_WINDOW_RADIUS: usize = 1;

/// An open reading session: package + user state + chapter window.
pub struct Session {
    pub id: String,
    package: KomaPackage<File>,
    package_path: PathBuf,
    chapter_ids: Vec<String>,
    current_chapter: String,
    window_radius: usize,
    /// Resident chapter bodies keyed by id.
    cache: HashMap<String, Chapter>,
    pub user_state: UserState,
    user_state_path: PathBuf,
}

impl Session {
    /// Open a `.koma` package and load (or create) user state under `state_dir`.
    pub fn open(
        package_path: impl AsRef<Path>,
        state_dir: impl AsRef<Path>,
    ) -> Result<Self, RuntimeError> {
        let package_path = package_path.as_ref().to_path_buf();
        let package = KomaPackage::open_file(&package_path)
            .map_err(|e| RuntimeError::Package(e.to_string()))?;
        let chapter_ids: Vec<String> = package.chapter_ids().map(str::to_owned).collect();
        if chapter_ids.is_empty() {
            return Err(RuntimeError::Package("package has no chapters".into()));
        }
        let package_key = package_key(&package, &package_path);
        let user_state_path = UserState::default_path(state_dir.as_ref(), &package_key);
        let user_state = if user_state_path.exists() {
            let mut s = UserState::load(&user_state_path)?;
            if !chapter_ids.iter().any(|id| id == &s.position.chapter_id) {
                s.position.chapter_id = chapter_ids[0].clone();
                s.position.page = 0;
            }
            s
        } else {
            UserState::new(package_key, chapter_ids[0].clone())
        };
        let current_chapter = user_state.position.chapter_id.clone();
        let mut session = Self {
            id: uuid::Uuid::new_v4().to_string(),
            package,
            package_path,
            chapter_ids,
            current_chapter,
            window_radius: DEFAULT_WINDOW_RADIUS,
            cache: HashMap::new(),
            user_state,
            user_state_path,
        };
        session.refresh_window()?;
        Ok(session)
    }

    pub fn package_path(&self) -> &Path {
        &self.package_path
    }

    pub fn chapter_ids(&self) -> &[String] {
        &self.chapter_ids
    }

    pub fn current_chapter_id(&self) -> &str {
        &self.current_chapter
    }

    pub fn window(&self) -> ChapterWindow {
        ChapterWindow::around(&self.chapter_ids, &self.current_chapter, self.window_radius)
            .expect("current chapter in spine")
    }

    /// Navigate to a chapter and refresh the resident window.
    pub fn go_to(&mut self, chapter_id: &str) -> Result<(), RuntimeError> {
        if !self.chapter_ids.iter().any(|id| id == chapter_id) {
            return Err(RuntimeError::ChapterNotFound(chapter_id.to_owned()));
        }
        self.current_chapter = chapter_id.to_owned();
        self.user_state.position.chapter_id = chapter_id.to_owned();
        self.user_state.position.page = 0;
        self.refresh_window()?;
        Ok(())
    }

    /// Get a cached chapter body (loads on demand within the window).
    pub fn chapter(&mut self, id: &str) -> Result<&Chapter, RuntimeError> {
        if !self.cache.contains_key(id) {
            let ch = self
                .package
                .chapter(id)
                .map_err(|e| RuntimeError::Package(e.to_string()))?;
            self.cache.insert(id.to_owned(), ch);
        }
        Ok(self.cache.get(id).expect("just inserted"))
    }

    pub fn current_chapter(&mut self) -> Result<&Chapter, RuntimeError> {
        let id = self.current_chapter.clone();
        self.chapter(&id)
    }

    pub fn scene(&mut self, chapter_id: &str) -> Result<Scene, RuntimeError> {
        self.package
            .scene(chapter_id)
            .map_err(|e| RuntimeError::Package(e.to_string()))
    }

    pub fn save_user_state(&self) -> Result<(), RuntimeError> {
        self.user_state.save(&self.user_state_path)
    }

    pub fn user_state_path(&self) -> &Path {
        &self.user_state_path
    }

    /// Whether a chapter body is currently resident in the window cache.
    pub fn is_resident(&self, chapter_id: &str) -> bool {
        self.cache.contains_key(chapter_id)
    }

    fn refresh_window(&mut self) -> Result<(), RuntimeError> {
        let window = self.window();
        // Evict chapters outside the window.
        self.cache.retain(|id, _| window.contains(id));
        // Prefetch missing window members.
        for id in &window.loaded {
            if !self.cache.contains_key(id) {
                let ch = self
                    .package
                    .chapter(id)
                    .map_err(|e| RuntimeError::Package(e.to_string()))?;
                self.cache.insert(id.clone(), ch);
            }
        }
        Ok(())
    }
}

fn package_key(pkg: &KomaPackage<File>, path: &Path) -> String {
    let m = pkg.manifest();
    let title = m.document.title.as_deref().unwrap_or("untitled");
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book".into());
    let raw = format!("{stem}-{}-{}", m.seed, title);
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Cursor;

    use koma_compiler::KomaCompiler;
    use koma_core::kir::{Block, Chapter, Document, Section, block};

    use super::*;

    fn sample_doc() -> Document {
        let mut doc = Document::new();
        doc.metadata = Some(koma_core::kir::DocumentMetadata {
            title: Some("The Long Dark".into()),
            author: Some("Big Dog".into()),
            language: Some("en".into()),
            identifiers: vec![],
            license: None,
            generator: "test".into(),
            seed: 42,
        });
        for i in 0..5 {
            doc.chapters.push(Chapter {
                id: format!("ch{i}"),
                title: Some(format!("Chapter {i}")),
                sections: vec![Section {
                    id: format!("s{i}"),
                    blocks: vec![Block {
                        kind: Some(block::Kind::Paragraph(koma_core::kir::paragraph(
                            format!("Text {i}"),
                        ))),
                    }],
                }],
            });
        }
        doc
    }

    fn write_package(dir: &Path) -> PathBuf {
        let doc = sample_doc();
        let path = dir.join("book.koma");
        let file = File::create(&path).unwrap();
        KomaCompiler
            .compile(&doc, &HashMap::new(), file)
            .expect("compile");
        path
    }

    #[test]
    fn opens_session_with_window_and_persists_state() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = write_package(dir.path());
        let state_dir = dir.path().join("state");
        let mut session = Session::open(&pkg, &state_dir).unwrap();
        assert_eq!(session.current_chapter_id(), "ch0");
        let w = session.window();
        assert!(w.loaded.contains(&"ch0".into()));
        assert!(w.loaded.contains(&"ch1".into()));

        session.go_to("ch2").unwrap();
        assert_eq!(session.current_chapter_id(), "ch2");
        let w = session.window();
        assert_eq!(
            w.loaded,
            vec!["ch1".to_string(), "ch2".to_string(), "ch3".to_string()]
        );
        assert!(session.is_resident("ch2"));
        assert!(!session.is_resident("ch0"));

        session.save_user_state().unwrap();
        let path = session.user_state_path().to_path_buf();
        drop(session);

        let session2 = Session::open(&pkg, &state_dir).unwrap();
        assert_eq!(session2.current_chapter_id(), "ch2");
        assert!(path.exists());
    }

    #[test]
    fn compile_bytes_roundtrip_helper() {
        // Keep Cursor-free compile path exercised for temp packages.
        let doc = sample_doc();
        let mut out = Cursor::new(Vec::new());
        KomaCompiler
            .compile(&doc, &HashMap::new(), &mut out)
            .unwrap();
        assert!(!out.into_inner().is_empty());
    }
}
