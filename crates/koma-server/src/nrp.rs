//! Narrative Rendering Protocol (NRP) v0.1 — Content API surface.
//!
//! Language-independent JSON request: "render / compile this document with
//! this context, theme, and preferences" (AGENTS.md Content API / Koma Protocol).
//! Designs toward a full protocol without replacing the binary Phase 8 routes.

use std::collections::HashMap;
use std::io::Cursor;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::http::StatusCode;
use axum::response::Response;
use koma_compiler::{KomaCompiler, KomaPackage};
use koma_core::kir::{Block, Chapter, Document, Section, block};
use koma_markdown::MarkdownAdapter;
use koma_renderer::{LayoutConfig, layout_config_from_scene, layout_config_from_theme};
use koma_scene::{SceneAnalysisHints, apply_chapter_analysis};
use serde::Deserialize;

use crate::error::ApiError;
use crate::handlers::{chapter_blocks, encode_png, parse_optional_theme, render_pages};

pub const NRP_VERSION: &str = "0.1.0";

/// Shared NRP request body for compile and render.
#[derive(Debug, Deserialize)]
pub struct NrpRequest {
    /// Must be `0.1.0`.
    pub version: String,
    pub document: NrpDocument,
    #[serde(default)]
    pub theme: Option<NrpTheme>,
    #[serde(default)]
    pub context: Option<NrpContext>,
    #[serde(default)]
    pub preferences: Option<NrpPreferences>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum NrpDocument {
    /// Markdown source (UTF-8).
    Markdown { content: String },
    /// Lightweight inline article / chapter paragraphs (no adapter needed).
    Inline {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        paragraphs: Vec<String>,
        #[serde(default)]
        chapters: Vec<NrpInlineChapter>,
    },
}

#[derive(Debug, Deserialize)]
pub struct NrpInlineChapter {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub paragraphs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NrpTheme {
    /// Full theme document as YAML (`theme-spec` v0.1 or v0.2).
    pub yaml: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct NrpContext {
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub mood: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NrpPreferences {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_backend")]
    pub backend: String,
    /// 0-based page index within the selected chapter.
    #[serde(default)]
    pub page: usize,
    /// Optional chapter id; default first.
    #[serde(default)]
    pub chapter: Option<String>,
    /// `off` | `low` | `medium` | `high` (v0.1: `off` strips effect layers).
    #[serde(default = "default_effects")]
    pub effects: String,
}

impl Default for NrpPreferences {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            backend: default_backend(),
            page: 0,
            chapter: None,
            effects: default_effects(),
        }
    }
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

fn default_effects() -> String {
    "medium".into()
}

/// `POST /nrp/v0.1/compile` — JSON body → `.koma` bytes.
pub async fn nrp_compile(Json(req): Json<NrpRequest>) -> Result<Bytes, ApiError> {
    validate_version(&req.version)?;
    let doc = document_from_request(&req.document)?;
    let theme = match &req.theme {
        Some(t) => parse_optional_theme(Some(&t.yaml))?,
        None => None,
    };
    let mut out = Cursor::new(Vec::new());
    KomaCompiler
        .compile_with_theme(&doc, &HashMap::new(), theme.as_ref(), &mut out)
        .map_err(|e| ApiError::bad_request(format!("compile failed: {e}")))?;
    Ok(Bytes::from(out.into_inner()))
}

/// `POST /nrp/v0.1/render` — JSON body → PNG (+ `X-Koma-Pages`, `X-Koma-Nrp-Version`).
pub async fn nrp_render(Json(req): Json<NrpRequest>) -> Result<Response, ApiError> {
    validate_version(&req.version)?;
    let prefs = req.preferences.unwrap_or_default();
    let doc = document_from_request(&req.document)?;
    let theme = match &req.theme {
        Some(t) => parse_optional_theme(Some(&t.yaml))?,
        None => None,
    };

    let mut out = Cursor::new(Vec::new());
    KomaCompiler
        .compile_with_theme(&doc, &HashMap::new(), theme.as_ref(), &mut out)
        .map_err(|e| ApiError::bad_request(format!("compile failed: {e}")))?;
    let bytes = out.into_inner();
    let mut pkg = KomaPackage::open(Cursor::new(bytes.as_slice()))
        .map_err(|e| ApiError::internal(format!("opening compiled package: {e}")))?;

    let id = match &prefs.chapter {
        Some(id) => id.clone(),
        None => pkg.chapter_ids().next().unwrap_or_default().to_owned(),
    };
    if id.is_empty() {
        return Err(ApiError::bad_request("document has no chapters"));
    }
    let ch = pkg
        .chapter(&id)
        .map_err(|e| ApiError::bad_request(format!("chapter `{id}`: {e}")))?;
    let mut scene = pkg
        .scene(&id)
        .map_err(|e| ApiError::bad_request(format!("scene `{id}`: {e}")))?;

    if let Some(ctx) = &req.context {
        apply_chapter_analysis(
            &mut scene,
            &SceneAnalysisHints {
                mood: ctx.mood.clone(),
                environment: ctx.environment.clone(),
            },
        );
    }
    if prefs.effects.eq_ignore_ascii_case("off") {
        strip_effect_layers(&mut scene);
    }

    let blocks = chapter_blocks(&ch);
    let package_theme = pkg
        .theme()
        .map_err(|e| ApiError::internal(format!("reading package theme: {e}")))?;
    let mut cfg = match theme.as_ref().or(package_theme.as_ref()) {
        Some(t) => {
            let mut cfg = layout_config_from_theme(t);
            cfg.width = prefs.width;
            cfg.height = prefs.height;
            cfg
        }
        None => LayoutConfig {
            width: prefs.width,
            height: prefs.height,
            ..Default::default()
        },
    };
    // Context-refined scene always wins for layout (presentation override).
    cfg = layout_config_from_scene(&scene, &cfg);

    let frames = render_pages(&blocks, &cfg, &prefs.backend)?;
    let frame = frames.get(prefs.page).ok_or_else(|| {
        ApiError::bad_request(format!(
            "page {} out of range (chapter has {} page{s})",
            prefs.page,
            frames.len(),
            s = if frames.len() == 1 { "" } else { "s" },
        ))
    })?;
    let png = encode_png(frame)?;
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "image/png")
        .header("x-koma-pages", frames.len().to_string())
        .header("x-koma-nrp-version", NRP_VERSION)
        .body(Body::from(png))
        .map_err(|e| ApiError::internal(format!("building response: {e}")))
}

fn validate_version(version: &str) -> Result<(), ApiError> {
    if version == NRP_VERSION {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "unsupported NRP version `{version}` (expected {NRP_VERSION})"
        )))
    }
}

fn document_from_request(doc: &NrpDocument) -> Result<Document, ApiError> {
    match doc {
        NrpDocument::Markdown { content } => MarkdownAdapter
            .to_kir_str(content, "nrp.md")
            .map_err(|e| ApiError::bad_request(format!("markdown parse failed: {e}"))),
        NrpDocument::Inline {
            title,
            paragraphs,
            chapters,
        } => Ok(inline_document(title.as_deref(), paragraphs, chapters)),
    }
}

fn inline_document(
    title: Option<&str>,
    paragraphs: &[String],
    chapters: &[NrpInlineChapter],
) -> Document {
    let mut doc = Document::new();
    doc.metadata = Some(koma_core::kir::DocumentMetadata {
        title: title.map(str::to_owned),
        author: None,
        language: None,
        identifiers: vec![],
        license: None,
        generator: "nrp-v0.1".into(),
        seed: 0,
    });

    if !chapters.is_empty() {
        for (i, ch) in chapters.iter().enumerate() {
            doc.chapters.push(chapter_from_paragraphs(
                &format!("ch{i}"),
                ch.title.as_deref(),
                &ch.paragraphs,
            ));
        }
    } else {
        doc.chapters.push(chapter_from_paragraphs(
            "ch0",
            title,
            paragraphs,
        ));
    }
    doc
}

fn chapter_from_paragraphs(id: &str, title: Option<&str>, paragraphs: &[String]) -> Chapter {
    let blocks: Vec<Block> = paragraphs
        .iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| Block {
            kind: Some(block::Kind::Paragraph(koma_core::kir::paragraph(p.clone()))),
        })
        .collect();
    Chapter {
        id: id.to_owned(),
        title: title.map(str::to_owned),
        sections: vec![Section {
            id: format!("{id}-s0"),
            blocks,
        }],
    }
}

fn strip_effect_layers(scene: &mut koma_scene::Scene) {
    scene
        .root
        .children
        .retain(|n| !matches!(n.kind, koma_scene::NodeKind::EffectLayer));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_builds_single_chapter() {
        let doc = inline_document(
            Some("Cold"),
            &["A cold coming.".into()],
            &[],
        );
        assert_eq!(doc.chapters.len(), 1);
        assert_eq!(doc.chapters[0].title.as_deref(), Some("Cold"));
    }
}
