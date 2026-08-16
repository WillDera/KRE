# Koma Rendering Engine — How It Works

This document explains the *implemented* engine: the compile pipeline, the
`.koma` runtime, scene synthesis, rendering backends, and how the CLI and
network service map onto the same library calls. It complements `DESIGN.md`
(the pre-code architecture target) and the format specs
(`kir-spec`, `koma-format-spec`, `theme-spec`, `scene-spec`,
`network-api-spec`, `plugin-api-spec`).

## 0. Mental model

KRE is three layers (AGENTS.md System Boundaries):

```
CONTENT SOURCE (EPUB, KIR, external app)
   │  adapter / bytes
   ▼
KIR  ── KomaCompiler ──►  .koma package (zip)
   │                        │
   │                        ▼
   └──► scenes synthesized   KomaPackage (lazy chapter/scene/theme loads)
        at compile time      │
                             ▼
                    layout (cosmic-text) ─► RenderBackend (software | wgpu) ─► RGBA frame ─► PNG
```

Key properties:

- **The runtime never parses source formats** and never synthesizes scenes —
  it only reads what the compiler produced.
- **Content is immutable.** Adapters read, the compiler encodes; nothing ever
  writes back to the source. Themes and scenes are *presentation truth*.
- **Everything is versioned and deterministic.** KIR, `.koma`, themes,
  scenes, the plugin API, and the network API each carry a version; identical
  inputs produce byte-identical `.koma` packages.
- **Lazy by design.** A package opens by reading only its manifest; chapters
  and scenes decode on demand, so million-word books stay within a small
  resident set (current + nearby content).

## 1. Content → KIR

Source formats are converted to the Koma Intermediate Representation (KIR)
by content adapters — plugins that implement `ContentAdapter`
(`koma_core::adapters`). The core engine has no knowledge of EPUB/PDF/HTML.

The shipped adapter is `koma_epub::EpubAdapter`:

1. Opens the zip, validates the `mimetype` (`application/epub+zip`).
2. Reads `META-INF/container.xml` → locates the OPF package.
3. Parses the OPF: metadata, manifest, spine.
4. Reads the ToC (EPUB3 `nav` or EPUB2 `NCX`).
5. Converts each spine XHTML item into a KIR `Chapter` — the natural
   lazy-loading and serialization boundary.

An external application can also *send KIR directly* (protobuf bytes) — the
engine is content-source agnostic. KIR is defined in
`docs/kir-spec-v0.1.md`; `Document::encode_document()/decode_document()` and
`Chapter::encode()/decode_chapter()` are the wire formats.

## 2. Compilation: KIR → `.koma`

`KomaCompiler::compile_with_theme(&doc, &assets, theme, writer)` in
`crates/koma-compiler/src/compiler.rs` produces a deterministic zip package.
Step by step:

1. **Validate** the KIR version (`validate_version`); reject unsupported
   schema versions.
2. **Validate and serialize the theme** if provided (a broken theme must not
   ship). Scenes are synthesized against the provided theme, or against
   `Theme::minimal("default")` when none is given.
3. **Sanitize ids** — chapter/scene ids are rejected if empty, `.`, `..`,
   or contain `/`, `\`, or NUL (untrusted-content path-traversal guard).
4. **Pre-encode chapters** into protobuf buffers (this also yields sizes for
   the manifest).
5. **Synthesize one scene per chapter** (`default_scene_for_chapter`) and
   serialize to JSON.
6. **Walk media ids** referenced by image/media blocks; only assets provided
   by the caller are embedded (unreferenced bytes are dropped).
7. **Build the manifest** — `format`, `version`, `generator` (name +
   version), `seed` (from KIR metadata, `0` default), document info
   (title/author/language/identifiers/license), chapter index (id/title/path/
   byte size), asset index, theme path, scene index.
8. **Write the zip** in a fixed order:
   - `koma.json` — the manifest
   - `chapters/{id}.ir` — chapter protobuf, one entry each
   - `assets/{id}` — embedded media
   - `theme.yaml` — the embedded theme (when present)
   - `scenes/{id}.json` — per-chapter scene JSON

**Determinism** (AGENTS.md Reproducibility): zip entries use a fixed
timestamp (`zip::DateTime::default()`), a fixed compression method, and
document order iteration. Scene synthesis sorts collection keys. Two compiles
of the same input are byte-identical, and the package records the generator
name/version plus the source seed so provenance is audit-able. The runtime
never depends on hidden AI state.

The `.koma` container layout is specified in `docs/koma-format-spec-v0.1.md`.

## 3. Scene synthesis

Scene generation is a **compiler** responsibility
(`crates/koma-scene/src/synthesis.rs`). `default_scene_for_chapter(id, theme,
blocks)` builds, deterministically:

- **Environment** — `abstract` kind, background from `theme.colors.background`,
  empty atmosphere.
- **Lighting** — ambient light with `theme.colors.text` and intensity `0.85`,
  no directional light.
- **Scene graph** — a `text` layer carrying the theme typography and an
  `open` animation; one `effects`-layer entry per enabled theme effect and
  per theme particle (with `density`/`speed`/`wind` folded into params).
- **Timeline** — an opening fade-in event sequence.
- **Entry transition** — `fade`, `duration 0.8s`.

Determinism rule: theme effect and particle map keys are **sorted** before
iteration, because `HashMap` iteration order is randomized per process. The
scene model (environment, lighting, node kinds, effects, transitions,
timeline actions) is documented in `docs/scene-spec-v0.1.md`. Scenes are
presentation truth — they never modify content.

## 4. The `.koma` runtime

`KomaPackage` (`crates/koma-compiler/src/package.rs`) is the runtime reader:

- **Open** reads only `koma.json` (the manifest) and validates format +
  version. It holds the `zip::ZipArchive` for on-demand reads.
- **`chapter(id)` / `chapter_by_index(i)`** — reads the zip entry and decodes
  the chapter protobuf lazily. Chapters load independently; opening a package
  never loads the whole book.
- **`validate()`** — decodes every chapter (the CLI `koma validate` uses
  this; fails fast on the first corrupt entry).
- **`theme()`** — lazily parses `theme.yaml` (None when not embedded).
- **`scene(id)`** — lazily reads `scenes/{id}.json` and parses the Scene.
- **`read_file(path)`** — internal; also used to read assets on demand.

The memory model matches AGENTS.md Book Processing Model: only the manifest
and the current + nearby chapters/scenes are resident. `koma-server` sessions
store package *bytes* and re-open on each request (a `KomaPackage` is not
`Clone` and holds a zip handle).

## 5. Rendering

Rendering is split into **layout** (backend-independent) and **draw**
(backend-specific), under a common `RenderBackend` abstraction in
`crates/koma-renderer`. The scene system has no dependency on wgpu or any
concrete backend.

### Layout (`layout.rs`)

`layout_blocks(&mut FontSystem, blocks, cfg)` shapes text with **cosmic-text**
and produces `PlacedLine`/`PlacedGlyph` items (with subpixel cache keys). It
depends on a `LayoutConfig`:

- `width` / `height` (device px)
- background color
- typography (font family/size/line-height/spacing/margin)
- font database (system fonts by default; an explicit `fontdb::Database`
  makes rendering deterministic for tests)

Two mapping helpers build the config:

- `layout_config_from_theme(&theme)` — maps the theme palette/typography onto
  the layout config.
- `layout_config_from_scene(&scene, &base)` — applies the chapter scene's
  environment background and text-layer typography on top of a base config.

**Theme/scene priority:** a caller-supplied theme (CLI `--theme`, server
`?theme=`) wins; otherwise the compiled scene is applied. This is the
presentation-truth rule — an author's explicit override beats synthesized
scenes.

### Software backend (`software/`)

Deterministic, portable CPU rasterizer:

1. `render_blocks` → `layout_blocks` → lines/glyphs.
2. `Frame::new(w, h)` filled with the background color.
3. Each glyph is rasterized from the font via **swash** (outline → alpha
   coverage) and composited into the RGBA frame.
4. Returns a `Frame { width, height, pixels }`.

GPU effects (particles, shaders, animation) are not supported here and
degrade to static output per AGENTS.md failure handling. This backend is the
testable path and the universal fallback.

### GPU backend (`gpu/`)

`WgpuBackend` (wgpu) is an offscreen, headless text renderer:

1. Creates a device/adapter (falls back to software when none is available).
2. **Glyph atlas** (`GlyphAtlas`): a shelf-packed texture that grows to a
   max dimension; glyphs are uploaded as they are encountered.
3. A textured-quad WGSL pipeline draws each glyph quad with subpixel
   positions into an offscreen render target.
4. The frame is read back to the CPU via a row-aligned staging buffer
   (`COPY_BYTES_PER_ROW_ALIGNMENT`) after `device.poll(...)` and `map_async`.

Supported: text + background (scene environment color). Particles/shader
effects/images degrade to static/background. The GPU and software backends
are **content-correct but not bit-identical** — tests assert content, not
exact pixels.

### PNG encoding

Frames encode to PNG (`png` crate, RGBA, 8-bit) — by the CLI (`koma render`),
the network service (`POST /render/document`), and in-process apps.

## 6. The three entry points call the same code

| Entry point | Compile | Render | Scenes |
|---|---|---|---|
| `koma` CLI (`tools/koma-cli`) | `KomaCompiler::compile_with_theme` | `render_blocks` + PNG | `KomaPackage::scene` |
| `koma serve` (`crates/koma-server`) | same, in `POST /compile/book` | same, in `POST /render/document` | same, in `GET /scene/state` |
| Embedded (FFI / Rust app) | same library calls | same library calls | same library calls |

The network service adds only: an HTTP layer (axum), in-memory session
handles (`POST /session/open`), EPUB upload spooling to a temp file (the
adapter is file-path based), and body-limit-free uploads. Everything else is
the engine library. That is why the "standalone server" is a deployment
choice, not a constraint — see `docs/network-api-spec-v0.1.md`.

## 7. Failure handling (AGENTS.md)

- GPU unavailable → `WgpuBackend::new()` errors → software fallback (CLI
  prints a notice; the server returns a software frame).
- No theme → `Theme::minimal("default")` and scenes still carry sane
  presentation.
- Missing assets → only referenced assets are embedded; renderers render
  text/background regardless.
- Broken/invalid input (KIR version, theme, ids, zip) → rejected at compile
  or open time with typed errors; the reading experience never fails because
  an optional system fails.

## 8. Data-flow summary

```
book.epub
   │ EpubAdapter (content truth: container/OPF/spine/nav/NCX)
   ▼
Document (KIR protobuf)
   │ KomaCompiler (validate versions, sanitize ids, encode chapters,
   │              synthesize scenes, walk assets, build manifest)
   ▼
book.koma  (zip: koma.json, chapters/*.ir, assets/*, theme.yaml, scenes/*.json)
   │ KomaPackage (open reads manifest only; chapter/scene/theme on demand)
   ▼
Chapter + Scene
   │ layout_blocks (cosmic-text) + layout_config_from_theme/scene
   ▼
RenderBackend ─► RGBA Frame ─► PNG  (software: swash; gpu: wgpu atlas + readback)
```

## See also

- `DESIGN.md` — pre-code architecture target and roadmap
- `README.md` — phases, workspace, quick start
- `docs/kir-spec-v0.1.md`, `docs/koma-format-spec-v0.1.md` — wire formats
- `docs/theme-spec-v0.1.md`, `docs/scene-spec-v0.1.md` — presentation formats
- `docs/network-api-spec-v0.1.md`, `docs/plugin-api-spec-v0.1.md` — interfaces
- `docs/koma-cli.md` — the local toolchain
- Integration guides: `docs/flutter-integration.md`,
  `docs/flutter-epub-pro-integration.md`, `docs/rust-rbook-integration.md`