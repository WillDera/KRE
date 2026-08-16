# Integrating KRE with a Rust app (rbook)

This guide shows how to use KRE from a Rust ebook app that reads EPUBs with
[rbook](https://crates.io/crates/rbook) (format-agnostic, EPUB 2/3 focused,
streaming reader). Because both sides are Rust, this is the **embedded
library usage** path (AGENTS.md: Integration API #1) — KRE crates link
directly into your app, no HTTP hop required.

```
Rust app
   ├─ rbook   = content truth  (metadata, spine, ToC, XHTML, resources)
   └─ KRE     = presentation   (EPUB -> KIR -> .koma -> scenes -> GPU frames)
```

The EPUB is immutable input; rbook and KRE parse it independently. rbook
stays the content layer (reading order, text, selection, accessibility);
KRE is the rendering layer (KIR compilation, scenes, offscreen GPU text
rendering).

## 1. Add KRE crates to your Cargo.toml

KRE is a Rust workspace, not yet on crates.io, so depend on it by path or
git:

```toml
[dependencies]
rbook = "0.7"
koma-core = { path = "vendor/koma/crates/koma-core" }
koma-epub = { path = "vendor/koma/crates/koma-adapters/koma-epub" }
koma-compiler = { path = "vendor/koma/crates/koma-compiler" }
koma-renderer = { path = "vendor/koma/crates/koma-renderer" }
koma-scene = { path = "vendor/koma/crates/koma-scene" }
koma-theme = { path = "vendor/koma/crates/koma-theme" }
png = "0.17"          # encode rendered frames
anyhow = "1"          # example only
```

or via git once the repository is published:

```toml
koma-core = { git = "https://github.com/<org>/koma_rendering_engine" }
# ...and so on for each crate
```

## 2. Minimal end-to-end flow

```rust
use std::collections::HashMap;
use std::io::Cursor;

use koma_compiler::{KomaCompiler, KomaPackage};
use koma_core::adapters::{ContentAdapter, ContentSource};
use koma_core::kir::{Block, block};
use koma_renderer::{LayoutConfig, SoftwareBackend, layout_config_from_scene};
use rbook::Epub;

fn render_first_chapter() -> anyhow::Result<()> {
    // 1. rbook = content truth: metadata, spine, ToC.
    let epub = Epub::open("book.epub")?;
    let title = epub.metadata().title().unwrap().value();
    println!("title: {title}");

    // 2. KRE compiles the same EPUB via its own adapter -> KIR -> .koma.
    //    (koma-epub reads a file path; spool uploaded bytes to a temp file
    //    if you only have an in-memory buffer.)
    let adapter = koma_epub::EpubAdapter;
    let doc = adapter.to_kir(&ContentSource::new("book.epub").with_format("epub"))?;
    let mut pkg_bytes = Cursor::new(Vec::new());
    KomaCompiler
        .compile_with_theme(&doc, &HashMap::new(), None, &mut pkg_bytes)?;

    // 3. Open the package; chapter ids come from KRE (spine order).
    let mut pkg = KomaPackage::open(Cursor::new(pkg_bytes.into_inner()))?;
    let id = pkg.chapter_ids().next().unwrap().to_owned();
    let ch = pkg.chapter(&id)?;
    let scene = pkg.scene(&id)?;

    // 4. Build display blocks: chapter title heading + section blocks.
    let mut blocks: Vec<Block> = Vec::new();
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

    // 5. Render (software backend; swap for WgpuBackend for GPU).
    let cfg = layout_config_from_scene(
        &scene,
        &LayoutConfig { width: 900, height: 1200, ..Default::default() },
    );
    let frame = SoftwareBackend::new().render_blocks(&blocks, &cfg)?;

    // 6. Encode PNG.
    let file = std::fs::File::create("chapter.png")?;
    let mut enc = png::Encoder::new(file, frame.width, frame.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&frame.pixels)?;
    Ok(())
}
```

## 3. Mapping rbook to KRE

| Concern | rbook | KRE |
|---|---|---|
| Metadata (title, creators, identifiers) | `epub.metadata()` | `/session/open` → `title`, package manifest |
| Reading order | `epub.spine()` / `epub.reader()` | `KomaPackage::chapter_ids()` |
| Table of contents | `epub.toc()` | embedded in the compiled scene graph |
| XHTML / text / resources | `epub.manifest()`, `read_resource_str()`, `read_resource_bytes()` | KIR chapter text, `.koma` resources |
| Scenes, effects, timeline | — | `KomaPackage::scene(id)` |
| Rendering (software / GPU) | — | `SoftwareBackend` / `WgpuBackend` |

For a navigation UI, keep rbook's spine/ToC as the index and use
`KomaPackage::chapter_ids()` for render/scene ids — both follow the EPUB
spine, so positions line up.

## 4. Alternative: HTTP service (process boundary)

If you prefer a stable language boundary (sidecar service, or the same
engine shared with a Flutter/web app), run `koma serve` and talk to it with
`ureq`/`reqwest`. The endpoints and flow are documented in
[flutter-integration.md](flutter-integration.md); only the HTTP client is
Rust instead of Dart. This keeps your app free of KRE build deps (wgpu,
font shaping, etc.).

## 5. Custom `rbook -> KIR` adapter (optional)

KRE's plugin architecture defines a content-adapter trait
(`koma_core::adapters::ContentAdapter`). If you want to skip KRE's own EPUB
parse and drive KIR directly from rbook, implement the trait:

```rust
struct RbookAdapter;

impl ContentAdapter for RbookAdapter {
    fn id(&self) -> &'static str { "koma-rbook" }

    fn supports(&self, source: &ContentSource) -> bool {
        source.format.as_deref() == Some("epub")
    }

    fn to_kir(&self, source: &ContentSource) -> Result<Document, AdapterError> {
        let epub = Epub::open(&source.uri)?;
        let mut doc = Document::new();
        // One KIR Chapter per spine entry; convert XHTML to KIR blocks.
        // (This duplicates the XHTML -> block work koma-epub already does;
        //  usually it is simpler to let the stock adapter parse.)
        Ok(doc)
    }
}
```

Note this means re-implementing the XHTML → KIR block conversion that
`koma-epub` already provides, so it is only worth it if your app needs to
pre-process or filter content before it reaches KIR.

## 6. Notes and limits

- **Memory.** `KomaPackage` loads chapters on demand
  (`pkg.chapter(&id)`); keep only the current + nearby chapters resident,
  mirroring KRE's lazy-loading model for million-word books.
- **Determinism.** Compilation is deterministic given the same source,
  compiler version, and theme; the GPU and software backends are
  content-correct, not bit-identical.
- **GPU.** `WgpuBackend::new()` creates an offscreen headless adapter; if it
  fails (no GPU), fall back to `SoftwareBackend` — rendering must never fail
  because an optional backend is missing.
- **Theme.** Pass a `koma_theme::Theme` to
  `compile_with_theme(&doc, &assets, Some(&theme), &mut out)` to embed it;
  an explicit theme override wins over the compiled scene.
- **Foreign formats.** rbook's format-agnostic design (future CBZ/FB2/MOBI)
  pairs naturally with KRE's content-adapter plugin interface — each source
  format gets an adapter that emits KIR, and the renderer never changes.