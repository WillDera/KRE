//! HTTP handlers for the Koma network service.

use std::collections::HashMap;
use std::io::{Cursor, Write};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use koma_compiler::{KomaCompiler, KomaPackage};
use koma_core::adapters::{ContentAdapter, ContentSource};
use koma_core::kir::{Block, Chapter, Document, block};
use koma_renderer::{
    Frame, LayoutConfig, SoftwareBackend, layout_config_from_scene, layout_config_from_theme,
};
use koma_scene::Scene;
use koma_theme::Theme;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::{ChapterInfo, ServerState, Session};

fn default_format() -> String {
    "auto".into()
}

fn default_width() -> u32 {
    800
}

fn default_height() -> u32 {
    1000
}

fn default_backend() -> String {
    "software".into()
}

/// Query parameters for `POST /compile/book`.
#[derive(Debug, Deserialize)]
pub struct CompileQuery {
    /// `auto` (default) | `epub` | `kir`. `auto` tries KIR, then EPUB.
    #[serde(default = "default_format")]
    pub format: String,
    /// Optional theme as a YAML string.
    pub theme: Option<String>,
}

/// Query parameters for `POST /render/document`.
#[derive(Debug, Deserialize)]
pub struct RenderQuery {
    pub chapter: Option<String>,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    /// `software` (default) | `gpu` (falls back to software).
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Optional theme override as a YAML string.
    pub theme: Option<String>,
}

/// Query parameters for `GET /scene/state`.
#[derive(Debug, Deserialize)]
pub struct SceneStateQuery {
    /// Session id from `POST /session/open`.
    pub session: String,
    pub chapter: Option<String>,
}

/// Response for `POST /session/open`.
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub title: Option<String>,
    pub chapters: Vec<ChapterInfo>,
}

/// `POST /compile/book` — body: EPUB or KIR bytes. Returns `.koma` bytes.
pub async fn compile_book(Query(q): Query<CompileQuery>, body: Bytes) -> Result<Bytes, ApiError> {
    let doc = match q.format.as_str() {
        "kir" => parse_kir(&body)?,
        "epub" => parse_epub(&body)?,
        "auto" | "" => match parse_kir(&body) {
            Ok(doc) => doc,
            Err(_) => parse_epub(&body)?,
        },
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown format `{other}` (expected auto | epub | kir)"
            )));
        }
    };
    let theme = parse_optional_theme(q.theme.as_deref())?;

    let mut out = std::io::Cursor::new(Vec::new());
    KomaCompiler
        .compile_with_theme(&doc, &HashMap::new(), theme.as_ref(), &mut out)
        .map_err(|e| ApiError::bad_request(format!("compile failed: {e}")))?;
    Ok(Bytes::from(out.into_inner()))
}

/// `POST /render/document` — body: `.koma` bytes. Returns a PNG frame.
pub async fn render_document(Query(q): Query<RenderQuery>, body: Bytes) -> Result<Bytes, ApiError> {
    let mut pkg = KomaPackage::open(Cursor::new(body.as_ref()))
        .map_err(|e| ApiError::bad_request(format!("not a .koma package: {e}")))?;

    let id = match &q.chapter {
        Some(id) => id.clone(),
        None => pkg.chapter_ids().next().unwrap_or_default().to_owned(),
    };
    let ch = pkg
        .chapter(&id)
        .map_err(|e| ApiError::bad_request(format!("chapter `{id}`: {e}")))?;
    let scene = pkg
        .scene(&id)
        .map_err(|e| ApiError::bad_request(format!("scene `{id}`: {e}")))?;

    let blocks = chapter_blocks(&ch);

    // Theme resolution: query override > package-embedded theme > defaults.
    let theme = match &q.theme {
        Some(_) => parse_optional_theme(q.theme.as_deref())?,
        None => pkg
            .theme()
            .map_err(|e| ApiError::internal(format!("reading package theme: {e}")))?,
    };
    let mut cfg = match &theme {
        Some(t) => {
            let mut cfg = layout_config_from_theme(t);
            cfg.width = q.width;
            cfg.height = q.height;
            cfg
        }
        None => LayoutConfig {
            width: q.width,
            height: q.height,
            ..Default::default()
        },
    };
    // Apply the chapter scene unless the caller overrode the theme.
    if q.theme.is_none() {
        cfg = layout_config_from_scene(&scene, &cfg);
    }

    let frame = render_frame(&blocks, &cfg, &q.backend)?;
    let png = encode_png(&frame)?;
    Ok(Bytes::from(png))
}

/// `POST /session/open` — body: `.koma` bytes. Returns a session id.
pub async fn open_session(
    State(state): State<ServerState>,
    body: Bytes,
) -> Result<Json<SessionInfo>, ApiError> {
    let pkg = KomaPackage::open(Cursor::new(body.as_ref()))
        .map_err(|e| ApiError::bad_request(format!("not a .koma package: {e}")))?;
    let manifest = pkg.manifest().clone();
    let chapters: Vec<ChapterInfo> = manifest
        .chapters
        .iter()
        .map(|c| ChapterInfo {
            id: c.id.clone(),
            title: c.title.clone(),
        })
        .collect();

    let session_id = state.insert(Session {
        bytes: body.to_vec(),
        title: manifest.document.title.clone(),
        chapters: chapters.clone(),
    });
    Ok(Json(SessionInfo {
        session_id,
        title: manifest.document.title,
        chapters,
    }))
}

/// `GET /scene/state?session=<id>&chapter=<id>` — returns the scene JSON.
pub async fn scene_state(
    State(state): State<ServerState>,
    Query(q): Query<SceneStateQuery>,
) -> Result<Json<Scene>, ApiError> {
    let bytes = state
        .get(&q.session)
        .ok_or_else(|| ApiError::not_found(format!("no session `{}`", q.session)))?;
    let mut pkg = KomaPackage::open(Cursor::new(bytes.as_slice()))
        .map_err(|e| ApiError::internal(format!("reopening session: {e}")))?;
    let id = match &q.chapter {
        Some(id) => id.clone(),
        None => pkg.chapter_ids().next().unwrap_or_default().to_owned(),
    };
    let scene = pkg
        .scene(&id)
        .map_err(|e| ApiError::bad_request(format!("scene `{id}`: {e}")))?;
    Ok(Json(scene))
}

/// Decode a KIR document from protobuf bytes.
fn parse_kir(bytes: &[u8]) -> Result<Document, ApiError> {
    let doc = Document::decode_document(bytes)
        .map_err(|e| ApiError::bad_request(format!("invalid KIR: {e}")))?;
    koma_core::error::validate_version(&doc.version)
        .map_err(|e| ApiError::bad_request(format!("unsupported KIR version: {e}")))?;
    Ok(doc)
}

/// Adapt an EPUB from uploaded bytes via a temp file (the adapter is
/// file-based by design).
fn parse_epub(bytes: &[u8]) -> Result<Document, ApiError> {
    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(bytes)?;
    let source = ContentSource::new(tmp.path().to_string_lossy().into_owned()).with_format("epub");
    koma_epub::EpubAdapter
        .to_kir(&source)
        .map_err(|e| ApiError::bad_request(format!("EPUB parse failed: {e}")))
}

fn parse_optional_theme(yaml: Option<&str>) -> Result<Option<Theme>, ApiError> {
    match yaml {
        Some(yaml) => {
            let theme = Theme::parse_yaml(yaml.as_bytes())
                .map_err(|e| ApiError::bad_request(format!("invalid theme: {e}")))?;
            theme
                .validate()
                .map_err(|e| ApiError::bad_request(format!("invalid theme: {e}")))?;
            Ok(Some(theme))
        }
        None => Ok(None),
    }
}

/// Render a chapter's blocks to a frame with the requested backend.
fn render_frame(blocks: &[Block], cfg: &LayoutConfig, backend: &str) -> Result<Frame, ApiError> {
    let mut software = SoftwareBackend::new();
    match backend {
        "gpu" => match koma_renderer::WgpuBackend::new() {
            Ok(mut gpu) => gpu
                .render_blocks(blocks, cfg)
                .map_err(|e| ApiError::internal(format!("gpu render failed: {e}"))),
            Err(e) => software.render_blocks(blocks, cfg).map_err(|err| {
                ApiError::internal(format!(
                    "gpu unavailable ({e}) and software fallback failed: {err}"
                ))
            }),
        },
        "software" => software
            .render_blocks(blocks, cfg)
            .map_err(|e| ApiError::internal(format!("render failed: {e}"))),
        other => Err(ApiError::bad_request(format!(
            "unknown backend `{other}` (expected software | gpu)"
        ))),
    }
}

/// Build the display blocks for a chapter: title heading + section blocks.
fn chapter_blocks(ch: &Chapter) -> Vec<Block> {
    let mut blocks = Vec::new();
    if let Some(title) = &ch.title {
        blocks.push(Block {
            kind: Some(block::Kind::Heading(koma_core::kir::Heading {
                level: 1,
                spans: vec![koma_core::kir::TextSpan {
                    text: title.clone(),
                    language: None,
                    style: None,
                }],
            })),
        });
    }
    for section in &ch.sections {
        blocks.extend(section.blocks.iter().cloned());
    }
    blocks
}

/// Encode a frame as a PNG.
fn encode_png(frame: &Frame) -> Result<Vec<u8>, ApiError> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| ApiError::internal(format!("png header: {e}")))?;
        writer
            .write_image_data(&frame.pixels)
            .map_err(|e| ApiError::internal(format!("png write: {e}")))?;
    }
    Ok(out)
}
