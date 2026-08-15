# .koma Package Format, Spec v0.1

Status: v0.1 (Phase 3). Canonical source: `crates/koma-compiler`.
Canonical encoding: **zip container**.

## 1. Purpose

The `.koma` package is the compiled representation of a document
(AGENTS.md: Compilation Model).

```
Content -> Koma Compiler -> .koma package -> Koma Runtime
```

The runtime must not perform expensive processing and must never load a whole
book into memory — therefore chapter bodies are separate protobuf entries
loaded on demand.

## 2. Layout

```
book.koma (zip)
├── koma.json            # package manifest (JSON)
├── chapters/<id>.ir     # protobuf-encoded Chapter (lazy load)
└── assets/<id>          # embedded asset bytes (optional, keyed by media id)
```

`koma.json` is the only entry read when a package is opened.

## 3. Package Manifest (`koma.json`)

```json
{
  "format": "koma",
  "version": "0.1.0",
  "generator": { "name": "koma-compiler", "version": "0.1.0" },
  "seed": 42,
  "document": {
    "title": "The Long Dark",
    "author": "Big Dog",
    "language": "en",
    "identifiers": [ { "scheme": "dc", "value": "urn:uuid:..." } ],
    "license": null
  },
  "chapters": [
    { "id": "ch1", "title": "Arrival", "path": "chapters/ch1.ir", "bytes": 83 }
  ],
  "assets": [
    { "id": "pic", "path": "assets/pic", "mime": null, "bytes": 1234 }
  ],
  "theme": null,
  "scene": null
}
```

- `format` must be `"koma"`; `version` must match the runtime's supported
  package format version. Breaking changes require version migration.
- `generator` + `seed` provide reproducibility: identical source content,
  compiler version, and configuration produce identical bytes.
- `theme` and `scene` are reserved slots (theme engine, scene graph phases).
- `assets[].path` is present only when bytes are embedded in the package;
  otherwise the entry records a media reference expected from an asset
  provider.

## 4. Chapters

Each `chapters/<id>.ir` is a protobuf-encoded KIR `Chapter` (the KIR
serialization boundary). Ids are validated against path traversal on compile.

## 5. Determinism

- Fixed zip entry timestamps.
- Stable entry order (manifest, chapters in spine order, assets in document
  order).
- No hidden state; `seed` is content-derived and recorded.

## 6. Reader contract

`koma-compiler::KomaPackage`:
- `open` — reads only `koma.json`.
- `chapter(id)` / `chapter_by_index(i)` — decodes one chapter on demand.
- `validate` — decodes every chapter, reporting the first failure.