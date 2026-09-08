//! The compiler: KIR `Document` -> deterministic `.koma` zip package.

use std::collections::{HashMap, HashSet};
use std::io::{Seek, Write};

use koma_analysis::{NarrativeAnalyzer, RuleBasedAnalyzer};
use koma_core::error::validate_version;
use koma_core::kir::{Document, block};
use koma_scene::{SceneAnalysisHints, default_scene_for_chapter_with_hints};
use koma_theme::Theme;
use prost::Message as _;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::manifest::{
    AssetEntry, ChapterEntry, DocumentInfo, GeneratorInfo, IdentifierInfo, KOMA_FORMAT,
    KOMA_PACKAGE_VERSION, LicenseInfo, PackageManifest, SceneEntry,
};

/// Zip entry containing the embedded theme (presentation truth).
pub const THEME_PATH: &str = "theme.yaml";
/// Zip entry containing compile-time semantic analysis.
pub const ANALYSIS_PATH: &str = "analysis.json";

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("invalid document: {0}")]
    InvalidDocument(String),
    #[error("invalid theme: {0}")]
    Theme(String),
    #[error("scene error: {0}")]
    Scene(String),
    #[error("analysis error: {0}")]
    Analysis(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(String),
    #[error("manifest serialization error: {0}")]
    Serialize(String),
}

impl From<zip::result::ZipError> for CompileError {
    fn from(e: zip::result::ZipError) -> Self {
        CompileError::Zip(e.to_string())
    }
}

/// Deterministic compiler for `.koma` packages.
pub struct KomaCompiler;

impl KomaCompiler {
    /// Compile a KIR `Document` (plus optional asset bytes keyed by media_id)
    /// into a `.koma` zip written to `writer`. Returns the package manifest.
    ///
    /// Runs the default rule-based analyzer (Phase 11).
    pub fn compile<W: Write + Seek>(
        &self,
        document: &Document,
        assets: &HashMap<String, Vec<u8>>,
        writer: W,
    ) -> Result<PackageManifest, CompileError> {
        self.compile_with_theme(document, assets, None, writer)
    }

    /// Compile with an optional embedded theme (`theme.yaml`).
    /// Runs the default rule-based analyzer.
    pub fn compile_with_theme<W: Write + Seek>(
        &self,
        document: &Document,
        assets: &HashMap<String, Vec<u8>>,
        theme: Option<&Theme>,
        writer: W,
    ) -> Result<PackageManifest, CompileError> {
        self.compile_with_analyzer(
            document,
            assets,
            theme,
            Some(&RuleBasedAnalyzer as &dyn NarrativeAnalyzer),
            writer,
        )
    }

    /// Compile with an optional theme and optional analyzer.
    ///
    /// Pass `analyzer: None` to skip analysis (scenes use theme defaults only;
    /// no `analysis.json` is embedded).
    pub fn compile_with_analyzer<W: Write + Seek>(
        &self,
        document: &Document,
        assets: &HashMap<String, Vec<u8>>,
        theme: Option<&Theme>,
        analyzer: Option<&dyn NarrativeAnalyzer>,
        writer: W,
    ) -> Result<PackageManifest, CompileError> {
        validate_version(&document.version)
            .map_err(|e| CompileError::InvalidDocument(e.to_string()))?;

        // Validate the theme before embedding (presentation truth is
        // versioned too; broken themes must not ship in a package).
        if let Some(t) = theme {
            t.validate()
                .map_err(|e| CompileError::Theme(e.to_string()))?;
        }
        let theme_yaml = theme
            .map(|t| t.to_yaml())
            .transpose()
            .map_err(|e| CompileError::Theme(e.to_string()))?;
        // Scenes are always generated; without a theme we synthesize against
        // a neutral default so environments still carry sane presentation.
        let scene_theme = theme.cloned().unwrap_or_else(|| Theme::minimal("default"));

        let analysis = analyzer.map(|a| a.analyze(document));
        let analysis_json = analysis
            .as_ref()
            .map(|a| a.to_json())
            .transpose()
            .map_err(|e| CompileError::Analysis(e.to_string()))?;

        // Pre-encode chapters (also yields sizes for the manifest) and
        // validate entry ids against path traversal.
        let mut chapters: Vec<(String, Vec<u8>)> = Vec::with_capacity(document.chapters.len());
        for chapter in &document.chapters {
            let id = sanitize_id(&chapter.id)?;
            let mut buf = Vec::new();
            chapter
                .encode(&mut buf)
                .map_err(|e| CompileError::InvalidDocument(e.to_string()))?;
            chapters.push((id, buf));
        }

        // Scene generation (compiler responsibility): one deterministic scene
        // per chapter, optionally refined by analysis hints.
        let scenes: Vec<(String, Vec<u8>)> = document
            .chapters
            .iter()
            .map(|chapter| {
                let id = sanitize_id(&chapter.id)?;
                let blocks: Vec<_> = chapter
                    .sections
                    .iter()
                    .flat_map(|s| s.blocks.iter())
                    .cloned()
                    .collect();
                let hints = analysis.as_ref().and_then(|a| {
                    a.chapter(&id).map(|ch| SceneAnalysisHints {
                        mood: ch.mood.clone(),
                        environment: ch.environment.clone(),
                    })
                });
                let scene =
                    default_scene_for_chapter_with_hints(&id, &scene_theme, &blocks, hints.as_ref());
                let json = scene
                    .to_json()
                    .map_err(|e| CompileError::Scene(e.to_string()))?;
                Ok((id, json.into_bytes()))
            })
            .collect::<Result<Vec<_>, CompileError>>()?;

        // Asset section: every media id referenced by the document; embed
        // bytes only when provided by the caller.
        let referenced = walk_media_ids(document);
        let embedded: HashMap<&String, &Vec<u8>> = referenced
            .iter()
            .filter_map(|id| assets.get(id).map(|b| (id, b)))
            .collect();

        let seed = document.metadata.as_ref().map(|m| m.seed).unwrap_or(0);
        let manifest = PackageManifest {
            format: KOMA_FORMAT.to_owned(),
            version: KOMA_PACKAGE_VERSION.to_owned(),
            generator: GeneratorInfo {
                name: "koma-compiler".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            seed,
            document: document_info(document),
            chapters: chapters
                .iter()
                .map(|(id, buf)| ChapterEntry {
                    id: id.clone(),
                    title: chapter_title(document, id),
                    path: chapter_path(id),
                    bytes: buf.len(),
                })
                .collect(),
            assets: referenced
                .iter()
                .map(|id| AssetEntry {
                    id: id.clone(),
                    path: embedded.get(id).map(|_| asset_path(id)),
                    mime: None,
                    bytes: embedded.get(id).map_or(0, |b| b.len()),
                })
                .collect(),
            theme: theme_yaml.as_ref().map(|_| THEME_PATH.to_owned()),
            analysis: analysis_json.as_ref().map(|_| ANALYSIS_PATH.to_owned()),
            scenes: scenes
                .iter()
                .map(|(id, buf)| SceneEntry {
                    id: id.clone(),
                    path: scene_path(id),
                    bytes: buf.len(),
                })
                .collect(),
        };

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| CompileError::Serialize(e.to_string()))?;

        let mut zip = ZipWriter::new(writer);
        zip.start_file(crate::manifest::MANIFEST_PATH, entry_options())?;
        zip.write_all(manifest_json.as_bytes())?;

        for (id, buf) in &chapters {
            zip.start_file(chapter_path(id), entry_options())?;
            zip.write_all(buf)?;
        }

        // Assets in document order for determinism.
        for (id, bytes) in &embedded {
            zip.start_file(asset_path(id), entry_options())?;
            zip.write_all(bytes)?;
        }

        if let Some(yaml) = &theme_yaml {
            zip.start_file(THEME_PATH, entry_options())?;
            zip.write_all(yaml.as_bytes())?;
        }

        if let Some(json) = &analysis_json {
            zip.start_file(ANALYSIS_PATH, entry_options())?;
            zip.write_all(json.as_bytes())?;
        }

        // Scenes in chapter order for determinism.
        for (id, json) in &scenes {
            zip.start_file(scene_path(id), entry_options())?;
            zip.write_all(json)?;
        }

        zip.finish()?;
        Ok(manifest)
    }
}

/// Deterministic zip entry options: fixed timestamp, no per-file state.
fn entry_options() -> FileOptions<'static, ()> {
    FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
}

fn chapter_path(id: &str) -> String {
    format!("chapters/{id}.ir")
}

fn asset_path(id: &str) -> String {
    format!("assets/{id}")
}

fn scene_path(id: &str) -> String {
    format!("scenes/{id}.json")
}

fn document_info(document: &Document) -> DocumentInfo {
    let meta = document.metadata.as_ref();
    DocumentInfo {
        title: meta.and_then(|m| m.title.clone()),
        author: meta.and_then(|m| m.author.clone()),
        language: meta.and_then(|m| m.language.clone()),
        identifiers: meta
            .map(|m| {
                m.identifiers
                    .iter()
                    .map(|i| IdentifierInfo {
                        scheme: i.scheme.clone(),
                        value: i.value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        license: meta.and_then(|m| m.license.as_ref()).map(|l| LicenseInfo {
            name: l.name.clone(),
            creator: l.creator.clone(),
            license: l.license.clone(),
            source: l.source.clone(),
            version: l.version.clone(),
        }),
    }
}

fn chapter_title(document: &Document, id: &str) -> Option<String> {
    document
        .chapters
        .iter()
        .find(|c| c.id == id)
        .and_then(|c| c.title.clone())
}

/// Media ids referenced by the document, in document order, deduplicated.
fn walk_media_ids(document: &Document) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for chapter in &document.chapters {
        for section in &chapter.sections {
            for block in &section.blocks {
                let id = match &block.kind {
                    Some(block::Kind::Image(i)) => Some(i.media_id.clone()),
                    Some(block::Kind::Media(m)) => Some(m.media_id.clone()),
                    _ => None,
                };
                if let Some(id) = id {
                    if seen.insert(id.clone()) {
                        out.push(id);
                    }
                }
            }
        }
    }
    out
}

/// Reject ids that could escape the package (untrusted content).
fn sanitize_id(id: &str) -> Result<String, CompileError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        return Err(CompileError::InvalidDocument(format!(
            "unsafe entry id `{id}`"
        )));
    }
    Ok(id.to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use koma_core::kir::{Block, Chapter, TextSpan, block};

    use super::*;
    use crate::package::{KomaPackage, PackageError};

    fn sample_document() -> Document {
        let mut doc = Document::new();
        doc.metadata = Some(koma_core::kir::DocumentMetadata {
            title: Some("The Long Dark".to_owned()),
            author: Some("Big Dog".to_owned()),
            language: Some("en".to_owned()),
            identifiers: vec![koma_core::kir::Identifier {
                scheme: "dc".to_owned(),
                value: "urn:uuid:1".to_owned(),
            }],
            license: None,
            generator: "test".to_owned(),
            seed: 42,
        });
        for i in 0..3 {
            let para = koma_core::kir::paragraph(format!("Chapter {i} text."));
            let blocks = vec![
                Block {
                    kind: Some(block::Kind::Heading(koma_core::kir::Heading {
                        level: 1,
                        spans: vec![TextSpan {
                            text: format!("Chapter {i}"),
                            language: None,
                            style: None,
                        }],
                    })),
                },
                Block {
                    kind: Some(block::Kind::Paragraph(para)),
                },
            ];
            doc.chapters.push(Chapter {
                id: format!("ch{i}"),
                title: Some(format!("Chapter {i}")),
                sections: vec![koma_core::kir::Section {
                    id: format!("sec{i}"),
                    blocks,
                }],
            });
        }
        doc
    }

    fn compile_to(doc: &Document, assets: &HashMap<String, Vec<u8>>) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        KomaCompiler
            .compile(doc, assets, &mut out)
            .expect("compile");
        out.into_inner()
    }

    #[test]
    fn round_trips_document() {
        let doc = sample_document();
        let bytes = compile_to(&doc, &HashMap::new());
        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");

        let m = pkg.manifest();
        assert_eq!(m.format, "koma");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.seed, 42);
        assert_eq!(m.chapters.len(), 3);
        assert_eq!(m.document.title.as_deref(), Some("The Long Dark"));

        pkg.validate().expect("validate");
        let ch1 = pkg.chapter("ch1").expect("chapter");
        assert_eq!(ch1, doc.chapters[1]);
        assert!(ch1.plain_text().contains("Chapter 1 text."));
    }

    #[test]
    fn chapters_load_lazily_and_independently() {
        let doc = sample_document();
        let bytes = compile_to(&doc, &HashMap::new());
        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");

        // Load only chapter 2; other chapters stay untouched in the archive.
        let ch2 = pkg.chapter_by_index(2).expect("chapter");
        assert_eq!(ch2, doc.chapters[2]);
    }

    #[test]
    fn output_is_deterministic() {
        let doc = sample_document();
        let assets = HashMap::from([("pic".to_owned(), vec![1, 2, 3, 4])]);
        let a = compile_to(&doc, &assets);
        let b = compile_to(&doc, &assets);
        assert_eq!(a, b);
    }

    #[test]
    fn embeds_referenced_assets() {
        let mut doc = sample_document();
        doc.chapters[0].sections[0].blocks.push(Block {
            kind: Some(block::Kind::Image(koma_core::kir::ImageBlock {
                media_id: "pic".to_owned(),
                alt: None,
                caption: None,
            })),
        });
        let assets = HashMap::from([
            ("pic".to_owned(), vec![1, 2, 3]),
            ("unused".to_owned(), vec![9]),
        ]);
        let bytes = compile_to(&doc, &assets);
        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");
        let assets_manifest = &pkg.manifest().assets;
        assert_eq!(assets_manifest.len(), 1);
        assert_eq!(assets_manifest[0].id, "pic");
        assert_eq!(assets_manifest[0].bytes, 3);
        // Unreferenced asset bytes are not embedded.
        assert!(pkg.read_file("assets/unused").is_err());
    }

    #[test]
    fn rejects_wrong_version() {
        let mut doc = sample_document();
        doc.version = "9.9.9".to_owned();
        let err = KomaCompiler
            .compile(&doc, &HashMap::new(), Cursor::new(Vec::new()))
            .expect_err("should fail");
        assert!(matches!(err, CompileError::InvalidDocument(_)));
    }

    #[test]
    fn rejects_unsafe_ids() {
        let mut doc = sample_document();
        doc.chapters[0].id = "../evil".to_owned();
        let err = KomaCompiler
            .compile(&doc, &HashMap::new(), Cursor::new(Vec::new()))
            .expect_err("should fail");
        assert!(matches!(err, CompileError::InvalidDocument(_)));
    }

    #[test]
    fn embeds_theme_and_sets_manifest_path() {
        let doc = sample_document();
        let theme = koma_theme::Theme::parse_yaml(
            br##"
version: "0.1.0"
name: Imperial Archive
colors:
  background: "#0b0e14"
"##
            .as_slice(),
        )
        .expect("parse");

        let mut out = Cursor::new(Vec::new());
        KomaCompiler
            .compile_with_theme(&doc, &HashMap::new(), Some(&theme), &mut out)
            .expect("compile");
        let bytes = out.into_inner();

        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");
        assert_eq!(
            pkg.manifest().theme.as_deref(),
            Some(crate::compiler::THEME_PATH)
        );
        let embedded = pkg.theme().expect("theme").expect("some theme");
        assert_eq!(embedded.name, "Imperial Archive");
        assert_eq!(
            embedded.colors.background,
            koma_theme::Color::rgb(0x0b, 0x0e, 0x14)
        );
    }

    #[test]
    fn theme_embedding_is_deterministic() {
        let doc = sample_document();
        let theme = koma_theme::Theme::minimal("Fixed");
        let build = || {
            let mut out = Cursor::new(Vec::new());
            KomaCompiler
                .compile_with_theme(&doc, &HashMap::new(), Some(&theme), &mut out)
                .expect("compile");
            out.into_inner()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn rejects_invalid_theme_version() {
        let doc = sample_document();
        let theme = koma_theme::Theme {
            version: "9.9.9".to_owned(),
            ..koma_theme::Theme::minimal("Bad")
        };
        let err = KomaCompiler
            .compile_with_theme(&doc, &HashMap::new(), Some(&theme), Cursor::new(Vec::new()))
            .expect_err("should fail");
        assert!(matches!(err, CompileError::Theme(_)));
    }

    #[test]
    fn no_theme_means_no_embedded_theme() {
        let doc = sample_document();
        let bytes = compile_to(&doc, &HashMap::new());
        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");
        assert!(pkg.manifest().theme.is_none());
        assert!(pkg.theme().expect("ok").is_none());
    }

    #[test]
    fn embeds_scenes_per_chapter() {
        let doc = sample_document();
        let bytes = compile_to(&doc, &HashMap::new());
        let pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");

        let manifest = pkg.manifest();
        assert_eq!(manifest.scenes.len(), doc.chapters.len());
        for entry in &manifest.scenes {
            assert!(entry.path.starts_with("scenes/"));
            assert!(entry.path.ends_with(".json"));
            assert!(entry.bytes > 0);
        }
    }

    #[test]
    fn scenes_load_lazily_and_are_valid() {
        let doc = sample_document();
        let bytes = compile_to(&doc, &HashMap::new());
        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");

        let scene = pkg.scene("ch1").expect("scene");
        scene.validate().expect("valid scene");
        assert_eq!(scene.chapter_id, "ch1");

        let err = pkg.scene("nope").expect_err("missing scene");
        assert!(matches!(err, PackageError::SceneNotFound(_)));
    }

    #[test]
    fn scene_synthesis_is_deterministic_in_package() {
        let doc = sample_document();
        let a = compile_to(&doc, &HashMap::new());
        let b = compile_to(&doc, &HashMap::new());
        assert_eq!(a, b);
    }

    #[test]
    fn embeds_analysis_and_refines_scene() {
        let mut doc = sample_document();
        doc.chapters[0].sections[0].blocks[1] = Block {
            kind: Some(block::Kind::Paragraph(koma_core::kir::paragraph(
                "The old inquisitor entered the frozen chamber under frost.",
            ))),
        };
        let bytes = compile_to(&doc, &HashMap::new());
        let mut pkg = KomaPackage::open(Cursor::new(bytes)).expect("open");
        assert_eq!(
            pkg.manifest().analysis.as_deref(),
            Some(crate::compiler::ANALYSIS_PATH)
        );
        let analysis = pkg.analysis().expect("ok").expect("some analysis");
        assert_eq!(analysis.generator, "koma-analysis-rules");
        let ch0 = analysis.chapter("ch0").expect("ch0");
        assert_eq!(ch0.mood.as_deref(), Some("ominous"));
        assert!(ch0.entities.iter().any(|e| e.name == "Inquisitor"));

        let scene = pkg.scene("ch0").expect("scene");
        assert_eq!(
            scene.environment.kind,
            koma_scene::EnvironmentKind::Indoor
        );
        assert!(scene.environment.atmosphere.contains_key("frost"));
    }

    #[test]
    fn no_analyzer_skips_analysis_json() {
        let doc = sample_document();
        let mut out = Cursor::new(Vec::new());
        KomaCompiler
            .compile_with_analyzer(&doc, &HashMap::new(), None, None, &mut out)
            .expect("compile");
        let mut pkg = KomaPackage::open(Cursor::new(out.into_inner())).expect("open");
        assert!(pkg.manifest().analysis.is_none());
        assert!(pkg.analysis().expect("ok").is_none());
    }
}
