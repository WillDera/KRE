# Narrative Rendering Protocol (NRP) — v0.1

NRP is the language-independent **Content API**: any application can say
“render this document with this context, this theme, and these preferences”
without depending on Koma crates or the binary Phase 8 upload routes.

This is a minimal, versioned sketch toward a future Narrative Rendering
Protocol (AGENTS.md). It does not replace `/compile/book` or
`/render/document`; those remain for raw EPUB / KIR / `.koma` bytes.

**Version:** `0.1.0`  
**Transport:** HTTP JSON over the Koma network service  
**Prefix:** `/nrp/v0.1/`

## Endpoints

| Endpoint | Method | Body | Returns |
|---|---|---|---|
| `/nrp/v0.1/compile` | POST | NRP JSON | `.koma` bytes (`application/octet-stream`) |
| `/nrp/v0.1/render` | POST | NRP JSON | PNG (`image/png`) |

**Errors:** non-2xx → `application/json` `{"error":"<message>"}` with
`400` / `500` as appropriate.

**Render headers:**

- `X-Koma-Pages` — total pages in the selected chapter
- `X-Koma-Nrp-Version` — `0.1.0`

## Request object

```json
{
  "version": "0.1.0",
  "document": { "...": "see Document formats" },
  "theme": { "yaml": "version: \"0.2.0\"\nname: Archive\n..." },
  "context": {
    "environment": "frozen",
    "mood": "ominous"
  },
  "preferences": {
    "width": 800,
    "height": 1000,
    "backend": "software",
    "page": 0,
    "chapter": null,
    "effects": "medium"
  }
}
```

| Field | Required | Notes |
|---|---|---|
| `version` | yes | Must be `0.1.0` |
| `document` | yes | See formats below |
| `theme` | no | `{ "yaml": "<theme document>" }` |
| `context` | no | Presentation hints only (never content truth) |
| `preferences` | no | Defaults shown above |

### Preferences

| Key | Default | Values |
|---|---|---|
| `width` / `height` | `800` / `1000` | frame size, px |
| `backend` | `software` | `software` \| `gpu` (GPU falls back to software) |
| `page` | `0` | 0-based page within chapter |
| `chapter` | first | chapter id after compile |
| `effects` | `medium` | `off` \| `low` \| `medium` \| `high` — v0.1: `off` strips effect layers; other levels reserved |

### Context

Presentation only. Applied via scene analysis hints after compile:

- `environment` — e.g. `space`, `indoor`, `outdoor`, `frozen`
- `mood` — e.g. `ominous`, `dark`, `warm`

Never modifies KIR content.

## Document formats

Tagged by `format`.

### `markdown`

```json
{
  "format": "markdown",
  "content": "---\ntitle: The Long Dark\n---\n\n# Arrival\n\nA cold coming."
}
```

Uses `koma-markdown` (front matter + H1 chapters).

### `inline`

Lightweight articles without an adapter — suitable for chat apps, Notion
bridges, games, AI assistants:

```json
{
  "format": "inline",
  "title": "A cold coming",
  "paragraphs": [
    "A cold coming we had of it.",
    "Just the worst time of the year."
  ]
}
```

Or multi-chapter:

```json
{
  "format": "inline",
  "title": "The Long Dark",
  "chapters": [
    { "title": "Arrival", "paragraphs": ["A cold coming."] },
    { "title": "Night", "paragraphs": ["The frost deepened."] }
  ]
}
```

If `chapters` is non-empty, top-level `paragraphs` are ignored.

## Examples

```bash
# Compile inline content to .koma
curl -X POST http://127.0.0.1:7878/nrp/v0.1/compile \
  -H 'content-type: application/json' \
  -d '{
    "version": "0.1.0",
    "document": {
      "format": "inline",
      "title": "Cold",
      "paragraphs": ["A cold coming."]
    }
  }' -o cold.koma

# Render with context + theme
curl -X POST http://127.0.0.1:7878/nrp/v0.1/render \
  -H 'content-type: application/json' \
  -d '{
    "version": "0.1.0",
    "document": {
      "format": "markdown",
      "content": "# Arrival\n\nA cold coming."
    },
    "context": { "environment": "frozen", "mood": "ominous" },
    "preferences": { "width": 900, "height": 1200, "effects": "off" }
  }' -o page.png
```

## Relationship to Phase 8 routes

| Need | Use |
|---|---|
| EPUB / KIR / Markdown **bytes** | `POST /compile/book` |
| Existing `.koma` PNG | `POST /render/document` |
| JSON document + theme + prefs | **`POST /nrp/v0.1/*`** |
| Long-lived package sessions | `POST /session/open` + `GET /scene/state` |

## Privacy

Same as the network API: content stays on the host; bind to localhost by
default; uploads are untrusted.

## See also

- Network API: `docs/network-api-spec-v0.1.md`
- Theme: `docs/theme-spec-v0.2.md`
- Scene: `docs/scene-spec-v0.1.md`
- KIR: `docs/kir-spec-v0.1.md`
