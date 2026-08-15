//! koma-compiler: KIR -> `.koma` package.
//!
//! Responsibilities (per AGENTS.md):
//! - optimization
//! - asset indexing
//! - scene generation (slots; populated in later phases)
//! - metadata compilation
//!
//! Output is a single-file zip container (like EPUB/JAR) with a JSON package
//! manifest and one protobuf-encoded file per chapter so the runtime can
//! stream chapters lazily and never load a whole book into memory.
//!
//! The output is deterministic: identical source content, compiler version,
//! and configuration produce identical bytes (fixed zip timestamps, stable
//! entry order, no hidden state).

pub mod compiler;
pub mod manifest;
pub mod package;

pub use compiler::{CompileError, KomaCompiler};
pub use manifest::{AssetEntry, ChapterEntry, DocumentInfo, GeneratorInfo, PackageManifest};
pub use package::{KomaPackage, PackageError};
