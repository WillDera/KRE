# KIR — Koma Intermediate Representation, Spec v0.1

Status: v0.1 (Phase 1). Canonical source of truth: `crates/koma-core/proto/kir.proto`.
This document mirrors the schema; when they diverge, the `.proto` file wins.

## 1. Purpose

KIR is the stable, versioned, language- and platform-independent document
model that sits between content sources and the renderer.

- Content adapters and external applications **produce** KIR.
- The renderer **consumes only** KIR. It never parses source formats.
- External applications (Python, Go, JS, C#) use the same `.proto` schema.

## 2. Encoding

- Canonical encoding: **Protocol Buffers** (protobuf wire format).
- Rust runtime/codegen: `prost` + `prost-build` (system `protoc`).
- Versioning: `Document.version` carries the schema version (`"0.1.0"`).
  Breaking changes require version migration.

## 3. Document Model

```
Document
├── version          string              "0.1.0"
├── metadata         DocumentMetadata
└── chapters[]       Chapter             (lazy-load boundary)
```

### Chapter = serialization boundary

The runtime must never load a whole document into memory. A `Chapter` is an
independent serializable unit (`Chapter::encode_chapter` /
`decode_chapter`), enabling:

- lazy loading
- chapter / section streaming
- memory eviction
- background loading

## 4. Content Model

```
Chapter  → sections[] → blocks[]
Block    = oneof: Paragraph | Heading | Image | Media | Quote | List
Paragraph → spans[] (TextSpan), annotations[] (Annotation), semantic (SemanticMetadata)
TextSpan → text + optional language override (BCP-47) + optional SpanStyle
```

- `TextSpan.language` drives shaping and bidi per span; the engine must not
  assume English.
- `SpanStyle` (bold/italic/underline/color) is presentation hint only.
- `Annotation` carries user-state-like marks (highlight / note / bookmark);
  real user state is stored separately from `.koma` packages.

## 5. Semantic Layer

`SemanticMetadata` carries meaning — never alters content truth:

- `entities[]` (e.g. character "Inquisitor")
- `locations[]` (e.g. place "Chamber")
- `themes[]`
- `mood` / `environment`

Example paragraph from an external app:

```json
{
  "type": "paragraph",
  "text": "A cold coming.",
  "metadata": { "mood": "ominous", "environment": "frozen" }
}
```

Semantic truth and presentation truth are distinct from content truth
(AGENTS.md: Source of Truth).

## 6. Reproducibility

`DocumentMetadata.generator` and `DocumentMetadata.seed` carry generator
information and deterministic seed values so that compiled output is
reproducible. The runtime never depends on hidden AI state.

## 7. Media

`ImageBlock` / `MediaBlock` reference assets by `media_id` (asset manifest
resolves the actual bytes). Content must not embed raw asset bytes in KIR.

## 8. Attribution

`Attribution` (name/creator/license/source/version) is supported on document
metadata and, by extension, on every asset type (fonts, images, themes,
plugins, AI-generated assets, external libraries) for attribution reports.