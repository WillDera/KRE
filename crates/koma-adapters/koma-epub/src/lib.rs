//! koma-epub: EPUB content adapter (EPUB -> KIR).
//!
//! Source formats exist as adapters; the core engine never knows EPUB and
//! the renderer consumes only KIR. This crate converts an EPUB2/EPUB3
//! package into a KIR [`Document`](koma_core::kir::Document).
//!
//! Scope (minimal functional system):
//! - container.xml -> OPF package discovery
//! - OPF metadata / manifest / spine
//! - EPUB3 `nav` and EPUB2 `NCX` table of contents
//! - XHTML content documents -> KIR blocks (headings, paragraphs, images,
//!   quotes, lists, styled text spans)
//!
//! Each spine item becomes a KIR [`Chapter`](koma_core::kir::Chapter) — the
//! natural lazy-loading boundary.

pub mod adapter;
pub mod container;
pub mod content;
pub mod package;
pub mod path;
pub mod toc;
pub mod xml;

pub use adapter::{EpubAdapter, EpubError};
