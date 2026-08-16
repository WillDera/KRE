# Koma Rendering Engine (KRE)

**"Unreal Engine for narrative content."**

A general-purpose **narrative rendering platform**. KRE transforms structured
narrative content into immersive, adaptive, GPU-accelerated reading
experiences. It is **not** an ebook reader — it is an engine that applications
embed or talk to over HTTP.

```
CONTENT SOURCE  ->  CONTENT ADAPTER  ->  KIR  ->  SEMANTIC ANALYSIS  ->  SCENE GRAPH  ->  RENDERING ENGINE  ->  GPU DISPLAY
```

- **Content-source agnostic** — EPUB, PDF, Markdown, HTML, web novels, game
  dialogue, AI-generated content. Source formats exist as adapters; the core
  never parses them.
- **Compiled model** — content is compiled into a reproducible `.koma`
  package; the runtime never performs expensive processing.
- **Presentation truth** — themes, scenes, effects, and AI interpretations
  never modify content. Imported content is immutable.
- **Language-independent API** — external apps (Flutter, Python, Go, JS, C#,
  games) integrate over the Koma network service.
- **Lazy by design** — chapters stream; only current + nearby content is
  resident, for million-word books.

The operating charter is [`AGENTS.md`](AGENTS.md) — read it before
contributing. Design notes live in [`DESIGN.md`](DESIGN.md).

## Status (phases)

| Phase | Deliverable | Crate | Status |
|---|---|---|---|
| 1 | KIR specification | `koma-core` | done |
| 2 | EPUB adapter | `koma-epub` | done |
| 3 | `.koma` compiler + CLI | `koma-compiler`, `koma-cli` | done |
| 4 | Text renderer (software) | `koma-renderer` | done |
| 5 | Theme engine | `koma-theme` | done |
| 6 | Scene graph | `koma-scene` | done |
| 7 | GPU renderer (wgpu) | `koma-renderer` | done |
| 8 | Network service | `koma-server` | done |
| 9 | AI-assisted compilation | — | next |

## Workspace

| Crate | Responsibility |
|---|---|
| `crates/koma-core` | shared types, KIR (protobuf), errors, plugin interfaces |
| `crates/koma-adapters/koma-epub` | EPUB → KIR content adapter |
| `crates/koma-compiler` | KIR → `.koma` packages (optimization, scenes, assets) |
| `crates/koma-scene` | scene graph model + deterministic synthesis |
| `crates/koma-theme` | versioned YAML theme model |
| `crates/koma-renderer` | layout + rendering (software + wgpu backends) |
| `crates/koma-server` | HTTP network service (axum) |
| `tools/koma-cli` | `koma` developer toolchain |

## Getting started

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
```

Quick start:

```bash
# Import + compile + render
./target/debug/koma import book.epub          # -> book.kir
./target/debug/koma compile book.kir          # -> book.koma
./target/debug/koma inspect book.koma         # manifest + chapter index
./target/debug/koma render book.koma          # -> chapter.png (software)
./target/debug/koma render book.koma --backend gpu   # wgpu, software fallback

# Serve the HTTP API for any external app
./target/debug/koma serve --addr 127.0.0.1:7878
```

See [`docs/koma-cli.md`](docs/koma-cli.md) for the full CLI reference and
[`docs/network-api-spec-v0.1.md`](docs/network-api-spec-v0.1.md) for the
HTTP API.

## Documentation

### Specifications (versioned formats)

| Document | Format |
|---|---|
| [`docs/kir-spec-v0.1.md`](docs/kir-spec-v0.1.md) | KIR document model |
| [`docs/koma-format-spec-v0.1.md`](docs/koma-format-spec-v0.1.md) | `.koma` package format |
| [`docs/theme-spec-v0.1.md`](docs/theme-spec-v0.1.md) | theme YAML format |
| [`docs/scene-spec-v0.1.md`](docs/scene-spec-v0.1.md) | scene graph JSON format |
| [`docs/network-api-spec-v0.1.md`](docs/network-api-spec-v0.1.md) | HTTP network API |
| [`docs/plugin-api-spec-v0.1.md`](docs/plugin-api-spec-v0.1.md) | plugin / adapter API |

### Integration guides

| Guide | Path |
|---|---|
| Flutter (epubx) | [`docs/flutter-integration.md`](docs/flutter-integration.md) |
| Flutter (epub_pro) | [`docs/flutter-epub-pro-integration.md`](docs/flutter-epub-pro-integration.md) |
| Rust (rbook, embedded) | [`docs/rust-rbook-integration.md`](docs/rust-rbook-integration.md) |

### Tooling

| Guide | Path |
|---|---|
| CLI reference | [`docs/koma-cli.md`](docs/koma-cli.md) |

### Internals

| Guide | Path |
|---|---|
| How the engine works | [`docs/engine-internals.md`](docs/engine-internals.md) |

## Design pillars

- **Source independence** — the renderer only consumes KIR; adapters are
  plugins.
- **Deterministic compilation** — same source + compiler + config ⇒ identical
  `.koma`. The runtime never depends on hidden AI state.
- **Three layers** — Engine (library), Runtime (host), Applications (UI).
  The engine has no UI; the runtime has no platform UI; apps talk to the
  runtime via APIs.
- **Failure handling** — AI unavailable ⇒ rule-based fallback; GPU
  unavailable ⇒ static rendering; missing assets ⇒ procedural fallback.
  Reading never fails because an optional system fails.
- **Accessibility first-class** — animations/effects/backgrounds disable
  cleanly; semantic structure is exposed for screen readers and navigation.
- **Privacy** — content is private by default; local processing always works;
  external AI requires explicit permission.

## Non-goals

No blockchain, NFTs, social networks, marketplaces, or cloud dependencies.
KRE is an engine, not a store.

## Repository layout

```
crates/          # library crates
  koma-core/
  koma-adapters/koma-epub/
  koma-compiler/
  koma-scene/
  koma-theme/
  koma-renderer/
  koma-server/
tools/koma-cli/  # `koma` binary
docs/            # specs + integration guides
AGENTS.md        # operating charter
DESIGN.md        # architecture design notes
```