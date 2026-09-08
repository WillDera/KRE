//! koma-markdown: Markdown content adapter (Markdown -> KIR).
//!
//! Source formats exist as adapters; the core never parses Markdown and the
//! renderer consumes only KIR. ATX `#` headings start new chapters (lazy-load
//! boundary). Optional YAML-like front matter supplies document metadata.

mod adapter;
mod convert;
mod front_matter;

pub use adapter::{MarkdownAdapter, MarkdownError};
