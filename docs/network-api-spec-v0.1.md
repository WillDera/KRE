# Network API Specification — v0.1

The Koma network service is the language-independent integration interface
(AGENTS.md: Integration API). A Python, Go, JavaScript, C#, or any other
application can compile, render, and query scenes over HTTP without depending
on Koma internals.

**Versioning:** the API is versioned; breaking changes require a version bump
and migration (AGENTS.md Compatibility).

## Running the service

```bash
./target/debug/koma serve --addr 127.0.0.1:7878
```

- Default bind: `127.0.0.1:7878` (localhost only — do not expose publicly;
  content and sessions are private by default).
- The service accepts arbitrarily large request bodies (body limit disabled)
  so 100MB EPUBs upload cleanly.
- Sessions are in-memory; restarting the service clears them.
- Content uploads are treated as untrusted input (validated before use).

## Endpoints

| Endpoint | Method | Body | Query | Returns |
|---|---|---|---|---|
| `/compile/book` | POST | EPUB or KIR bytes | `format`, `theme` | `.koma` bytes |
| `/render/document` | POST | `.koma` bytes | `chapter`, `width`, `height`, `backend`, `theme` | PNG bytes |
| `/session/open` | POST | `.koma` bytes | — | JSON session |
| `/scene/state` | GET | — | `session`, `chapter` | Scene JSON |

**Errors:** non-2xx responses return `Content-Type: application/json` with
`{"error": "<message>"}` and a meaningful HTTP status (`400` bad input /
invalid package, `404` unknown session, `500` internal).

**Binary responses** (`.koma`, PNG) use `Content-Type: application/octet-stream`.

---

### `POST /compile/book`

Compile a source document into a `.koma` package.

- **Body:** raw bytes — either an EPUB file or a serialized KIR document.
- **Query:**
  - `format` — `auto` (default) \| `epub` \| `kir`. `auto` tries KIR first,
    then EPUB.
  - `theme` — optional theme as a YAML string; embedded in the package.
- **Success:** `200` with `.koma` bytes.
- **Errors:** `400` — invalid KIR/EPUB, unsupported KIR version, invalid
  theme, or compile failure.

```
curl -X POST --data-binary @book.epub \
     "http://127.0.0.1:7878/compile/book?format=epub" -o book.koma
```

### `POST /render/document`

Render one chapter of a `.koma` package to a PNG frame.

- **Body:** raw `.koma` bytes.
- **Query:**
  - `chapter` — chapter id (default: first chapter).
  - `width` — frame width, px (default `800`).
  - `height` — frame height, px (default `1000`).
  - `backend` — `software` (default) \| `gpu`. GPU falls back to software
    when no adapter is available (AGENTS.md failure handling).
  - `theme` — optional theme override as a YAML string; wins over the
    package scene. Omit to use the compiled scene.
- **Success:** `200` with PNG bytes.
- **Errors:** `400` — not a `.koma` package / unknown chapter or scene;
  `500` — render failure (or GPU unavailable *and* software fallback failed).

```
curl -X POST --data-binary @book.koma \
     "http://127.0.0.1:7878/render/document?width=900&height=1200&backend=gpu" \
     -o chapter.png
```

### `POST /session/open`

Open a `.koma` package as an in-memory session (used by `/scene/state`).

- **Body:** raw `.koma` bytes.
- **Success:** `200` JSON:

```json
{
  "session_id": "koma-00000000",
  "title": "The Long Dark",
  "chapters": [ { "id": "ch1", "title": "Arrival" } ]
}
```

- **Errors:** `400` — not a `.koma` package.

### `GET /scene/state`

Return the compiled scene JSON for a chapter (environment, lighting, scene
graph, transitions, timeline — see `scene-spec-v0.1.md`).

- **Query:**
  - `session` — session id from `/session/open` (REQUIRED).
  - `chapter` — chapter id (default: first chapter).
- **Success:** `200` with the Scene JSON (`version`, `chapter_id`,
  `environment`, `lighting`, `root`, `transitions`, `timeline`).
- **Errors:** `404` — unknown session; `400` — unknown chapter/scene.

```
curl "http://127.0.0.1:7878/scene/state?session=koma-00000000"
```

## Content API example

The service accepts KIR directly, so an external application can push content
dynamically without an EPUB at all:

```
# An application constructs a KIR document (any language) ...
curl -X POST --data-binary @document.kir \
     "http://127.0.0.1:7878/compile/book?format=kir" -o doc.koma

curl -X POST --data-binary @doc.koma \
     "http://127.0.0.1:7878/session/open"
```

A third-party app can build a KIR document with its own tooling and render
it immediately — the engine is content-source agnostic.

## Chapter ids

Chapter ids returned by `/session/open` and consumed by `/render/document`
and `/scene/state` are the compiled package's chapter ids (EPUB spine order).
Use the session's `chapters` array as the navigation index.

## Theme override

The `theme` query parameter (both compile and render) takes a full theme as
a YAML string (`theme-spec-v0.1.md`). Compile embeds it; render applies it
immediately. An explicit override always wins over the compiled scene.

## Privacy & security

- Content never leaves the host except over the localhost socket; the engine
  transmits no books, reading history, or preferences.
- Bind to `127.0.0.1` in production; put the service behind an authenticated
  proxy if remote access is ever required.
- Uploaded content is untrusted: malformed KIR/EPUB/`.koma` inputs are
  rejected with `400`, never crash the service.

## Client libraries

The endpoint surface is stable; any HTTP client works. For reference
implementations see:

- Flutter (epubx): `docs/flutter-integration.md`
- Flutter (epub_pro): `docs/flutter-epub-pro-integration.md`
- Rust (rbook, embedded): `docs/rust-rbook-integration.md`

## See also

- CLI toolchain: `docs/koma-cli.md` (same operations as a local binary)
- NRP / Content API: `docs/nrp-spec-v0.1.md`
- `.koma` package format: `docs/koma-format-spec-v0.1.md`
- Scene JSON: `docs/scene-spec-v0.1.md`
- Theme YAML: `docs/theme-spec-v0.1.md`