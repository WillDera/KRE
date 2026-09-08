//! `.koma` package reader: manifest + lazy, per-chapter decoding.
//!
//! Opening a package reads only `koma.json` (metadata + chapter index).
//! Chapter bodies are protobuf files loaded on demand via [`KomaPackage::chapter`]
//! — the runtime never loads the whole book into memory.

use std::io::{Read, Seek};
use std::path::Path;

use koma_core::kir::Chapter;
use koma_scene::Scene;
use koma_theme::Theme;

use crate::manifest::{KOMA_FORMAT, KOMA_PACKAGE_VERSION, MANIFEST_PATH, PackageManifest};

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("not a .koma package: {0}")]
    NotKoma(String),
    #[error("unsupported package version `{0}` (supported: {1})")]
    UnsupportedVersion(String, &'static str),
    #[error("invalid package manifest: {0}")]
    InvalidManifest(String),
    #[error("chapter `{0}` not found")]
    ChapterNotFound(String),
    #[error("scene `{0}` not found")]
    SceneNotFound(String),
    #[error("theme error: {0}")]
    Theme(String),
    #[error("scene error: {0}")]
    Scene(String),
    #[error("analysis error: {0}")]
    Analysis(String),
    #[error("KIR decode error: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("zip error: {0}")]
    Zip(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<zip::result::ZipError> for PackageError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::FileNotFound => {
                PackageError::NotKoma(format!("missing `{MANIFEST_PATH}`"))
            }
            other => PackageError::Zip(other.to_string()),
        }
    }
}

impl From<koma_core::KirError> for PackageError {
    fn from(e: koma_core::KirError) -> Self {
        match e {
            koma_core::KirError::Decode(d) => PackageError::Decode(d),
            other => PackageError::InvalidManifest(other.to_string()),
        }
    }
}

/// An opened `.koma` package. Holds the zip archive for on-demand reads.
pub struct KomaPackage<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
    manifest: PackageManifest,
}

impl<R: Read + Seek> KomaPackage<R> {
    /// Open a package, reading only the manifest.
    pub fn open(reader: R) -> Result<Self, PackageError> {
        let mut archive = zip::ZipArchive::new(reader)?;
        let mut manifest_bytes = Vec::new();
        archive
            .by_name(MANIFEST_PATH)?
            .read_to_end(&mut manifest_bytes)?;
        let manifest: PackageManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| PackageError::InvalidManifest(e.to_string()))?;
        if manifest.format != KOMA_FORMAT {
            return Err(PackageError::NotKoma(format!(
                "unexpected format `{}`",
                manifest.format
            )));
        }
        if manifest.version != KOMA_PACKAGE_VERSION {
            return Err(PackageError::UnsupportedVersion(
                manifest.version,
                KOMA_PACKAGE_VERSION,
            ));
        }
        Ok(Self { archive, manifest })
    }

    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }

    pub fn chapter_count(&self) -> usize {
        self.manifest.chapters.len()
    }

    pub fn chapter_ids(&self) -> impl Iterator<Item = &str> {
        self.manifest.chapters.iter().map(|c| c.id.as_str())
    }

    /// Lazily decode a chapter by id.
    pub fn chapter(&mut self, id: &str) -> Result<Chapter, PackageError> {
        let path = self
            .manifest
            .chapters
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.path.clone())
            .ok_or_else(|| PackageError::ChapterNotFound(id.to_owned()))?;
        let bytes = self.read_file(&path)?;
        Ok(Chapter::decode_chapter(&bytes)?)
    }

    /// Lazily decode a chapter by spine index.
    pub fn chapter_by_index(&mut self, index: usize) -> Result<Chapter, PackageError> {
        let path = self
            .manifest
            .chapters
            .get(index)
            .map(|c| c.path.clone())
            .ok_or_else(|| PackageError::ChapterNotFound(format!("index {index}")))?;
        let bytes = self.read_file(&path)?;
        Ok(Chapter::decode_chapter(&bytes)?)
    }

    /// Decode every chapter; fails on the first corrupt entry.
    pub fn validate(&mut self) -> Result<(), PackageError> {
        let ids: Vec<String> = self
            .manifest
            .chapters
            .iter()
            .map(|c| c.id.clone())
            .collect();
        for id in ids {
            self.chapter(&id)?;
        }
        Ok(())
    }

    /// The theme embedded in this package, if any.
    pub fn theme(&mut self) -> Result<Option<Theme>, PackageError> {
        let Some(path) = self.manifest.theme.clone() else {
            return Ok(None);
        };
        let bytes = self.read_file(&path)?;
        Theme::parse_yaml(&bytes)
            .map(Some)
            .map_err(|e| PackageError::Theme(e.to_string()))
    }

    /// Lazily load the scene for a chapter by id.
    pub fn scene(&mut self, id: &str) -> Result<Scene, PackageError> {
        let path = self
            .manifest
            .scenes
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.path.clone())
            .ok_or_else(|| PackageError::SceneNotFound(id.to_owned()))?;
        let bytes = self.read_file(&path)?;
        Scene::parse_json(&bytes).map_err(|e| PackageError::Scene(e.to_string()))
    }

    /// Compile-time analysis embedded in this package, if any.
    pub fn analysis(&mut self) -> Result<Option<koma_analysis::AnalysisResult>, PackageError> {
        let Some(path) = self.manifest.analysis.clone() else {
            return Ok(None);
        };
        let bytes = self.read_file(&path)?;
        koma_analysis::AnalysisResult::from_json(&bytes)
            .map(Some)
            .map_err(|e| PackageError::Analysis(e.to_string()))
    }

    pub fn scene_count(&self) -> usize {
        self.manifest.scenes.len()
    }

    pub(crate) fn read_file(&mut self, path: &str) -> Result<Vec<u8>, PackageError> {
        let mut f = self.archive.by_name(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl KomaPackage<std::fs::File> {
    /// Open a `.koma` package from a file path.
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self, PackageError> {
        Self::open(std::fs::File::open(path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn rejects_zip_without_manifest() {
        // A zip with no koma.json must be reported as "not a koma package".
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("random.txt", opts).unwrap();
            use std::io::Write;
            w.write_all(b"hi").unwrap();
            w.finish().unwrap();
        }
        let err = KomaPackage::open(Cursor::new(buf.into_inner()))
            .err()
            .expect("should fail");
        assert!(matches!(err, PackageError::NotKoma(_)));
    }

    #[test]
    fn rejects_wrong_package_version() {
        let manifest = PackageManifest {
            format: "koma".to_owned(),
            version: "0.0.1".to_owned(),
            generator: crate::manifest::GeneratorInfo {
                name: "x".to_owned(),
                version: "0".to_owned(),
            },
            seed: 0,
            document: crate::manifest::DocumentInfo {
                title: None,
                author: None,
                language: None,
                identifiers: Vec::new(),
                license: None,
            },
            chapters: Vec::new(),
            assets: Vec::new(),
            theme: None,
            analysis: None,
            scenes: Vec::new(),
        };
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file(MANIFEST_PATH, opts).unwrap();
            use std::io::Write;
            w.write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            w.finish().unwrap();
        }
        let err = KomaPackage::open(Cursor::new(buf.into_inner()))
            .err()
            .expect("should fail");
        assert!(matches!(
            err,
            PackageError::UnsupportedVersion(_, KOMA_PACKAGE_VERSION)
        ));
    }
}
