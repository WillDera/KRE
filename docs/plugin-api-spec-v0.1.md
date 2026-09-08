# Plugin API Specification — v0.1

Koma supports plugins: content adapters, analyzers, render effects, themes,
asset providers, and exporters. Plugins communicate through stable,
versioned interfaces and are dynamically loadable where supported. A third
party can build `koma-notion-plugin`, `koma-github-plugin`,
`koma-wikipedia-plugin` without modifying Koma core.

**Versioning:** the plugin API is versioned; breaking changes require a
version bump and migration (AGENTS.md Compatibility).

## Plugin model

```rust
/// Stable plugin identifier.
pub type PluginId = &'static str;

/// Categories of plugins.
pub enum PluginKind {
    ContentAdapter,
    Analyzer,
    RenderEffect,
    Theme,
    AssetProvider,
    Exporter,
}

/// Permissions a plugin requests. The user must approve them
/// (AGENTS.md: Plugin Security).
pub struct PermissionSet {
    pub filesystem: bool,
    pub network: bool,
    pub gpu: bool,
    pub external_apis: bool,
}

/// Declared metadata for a plugin.
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub kinds: Vec<PluginKind>,
    pub permissions: PermissionSet,
}

/// Common interface implemented by every plugin.
pub trait Plugin {
    fn manifest(&self) -> &PluginManifest;
}
```

Plugins must declare their `PermissionSet`; the runtime host presents them
to the user for approval before granting filesystem, network, GPU, or
external-API access. `PermissionSet::none()` is the safe default.

## Content adapters

Content adapters convert external source formats into KIR (the core engine
MUST NOT know about any specific format).

```rust
/// A reference to external content to be adapted into KIR.
pub struct ContentSource {
    /// Location of the source (file path, URL, or logical name).
    pub uri: String,
    /// Format hint, e.g. "epub", "pdf", "markdown", "html".
    pub format: Option<String>,
}

/// Converts a source format into KIR.
pub trait ContentAdapter {
    /// Stable identifier used for plugin registration, e.g. "koma-epub".
    fn id(&self) -> &'static str;

    /// Whether this adapter can handle the given source.
    fn supports(&self, source: &ContentSource) -> bool;

    /// Convert the source into a KIR Document.
    fn to_kir(&self, source: &ContentSource) -> Result<Document, AdapterError>;
}
```

`ContentSource::new(uri)` builds a source with no format hint;
`.with_format("epub")` sets one. Adapters may inspect content directly and
ignore the hint.

### Errors

```rust
pub enum AdapterError {
    UnsupportedSource(String, String), // adapter `x` does not support source `y`
    Parse(String, String),             // parse error in `x`: `y`
    MissingResource(String),
    InvalidKir(String),                // adapter produced invalid KIR
    Io(#[from] std::io::Error),
}
```

### Writing an adapter

Implement `ContentAdapter`, then register it with the runtime host (the
runtime is built in a later phase; the interface is already stable).

```rust
struct MyFormatAdapter;

impl ContentAdapter for MyFormatAdapter {
    fn id(&self) -> &'static str { "koma-myformat" }
    fn supports(&self, source: &ContentSource) -> bool {
        source.format.as_deref() == Some("myformat")
            || source.uri.to_lowercase().ends_with(".myformat")
    }
    fn to_kir(&self, source: &ContentSource) -> Result<Document, AdapterError> {
        // ... build a KIR Document from the source ...
        Ok(doc)
    }
}
```

Any `ContentAdapter` automatically implements `Plugin` (a default manifest is
derived: id, name, version from the crate version, kind
`ContentAdapter`, no permissions). Adapter authors declare explicit
id/version/permissions via the manifest builder introduced with the runtime
host.

### Reference implementation

`koma-epub` (`koma_epub::EpubAdapter`) is the shipped EPUB adapter. It
discovers `container.xml` → OPF, parses metadata/manifest/spine, reads EPUB3
`nav` and EPUB2 `NCX` for the ToC, and converts each spine XHTML item into a
KIR `Chapter` (the lazy-loading boundary). Its `to_kir` reads from a file
path (`ContentSource::uri`).

## Analyzers

Analyzers produce semantic metadata from KIR. Their output is **compilation
data only** — the runtime never calls analyzers or external models.

```rust
trait NarrativeAnalyzer {
    fn id(&self) -> &'static str;
    fn analyze(&self, document: &Document) -> AnalysisResult;
}
```

Shipped in `koma-analysis` (Phase 11):

- `RuleBasedAnalyzer` — deterministic lexicon matching (default at compile).
- `AssistedAnalyzer` — same trait; currently delegates to rules. Future
  external backends must fall back to rules on error or denial.

Analysis embeds as `analysis.json` in `.koma` packages and refines scene
synthesis (environment kind, atmosphere). It never mutates KIR plain text.
Use `koma compile --no-analyze` to skip.

## Other plugin kinds

| Kind | Purpose |
|---|---|
| `RenderEffect` | procedural GPU effects (snow, rain, fog, light rays) |
| `Theme` | packaged themes (see `theme-spec-v0.1.md`) |
| `AssetProvider` | supply fonts, images, audio to the engine |
| `Exporter` | export scenes to video, screenshots, web components, HTML, accessibility tree, PDF |

The `PluginKind` enum reserves these categories; their interfaces ship with
the runtime host in later phases.

## Security model

- Plugins run with **controlled permissions** (filesystem, network, GPU,
  external APIs) that the user approves explicitly.
- External content is untrusted: inputs are validated, malformed sources are
  rejected, and assets are loaded safely (AGENTS.md Security).
- The core engine remains safe without plugins and without AI.

## Compatibility & stability

- The plugin interface is versioned; breaking changes require migration.
- The core engine never depends on a specific plugin. A company plugin (e.g.
  koma-notion-plugin) replaces or augments an adapter, not the engine.
- Dynamic loading is supported where the platform allows (dylib/WASM);
  static registration is always available.

## See also

- KIR document model: `kir-spec-v0.1.md`
- Scene graph format: `scene-spec-v0.1.md`
- Theme format: `theme-spec-v0.1.md`
- Rust rbook integration (custom adapter example): `rust-rbook-integration.md`