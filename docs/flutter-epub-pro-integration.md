# Integrating KRE with Flutter (epub_pro)

This guide shows how to use KRE from a Flutter ebook app that reads EPUBs
with [epub_pro](https://pub.dev/packages/epub_pro) — the actively maintained
Dart EPUB parser (a fork of `dart-epub`). It mirrors
[Integrating KRE with Flutter (epubx)](flutter-integration.md); the network
client is identical, only the content-parsing API differs.

```
Flutter app (epub_pro parses EPUB)
      │  EPUB bytes / .koma bytes
      ▼
 koma serve  (POST /compile/book, POST /render/document,
              POST /session/open, GET /scene/state)
      │
      ▼
 .koma package · PNG frames · scene JSON
```

## 1. Run the service

```bash
./target/debug/koma serve --addr 127.0.0.1:7878
```

Build it with `cargo build -p koma-cli` from the KRE workspace. The service
binds localhost only; keep it off public interfaces.

## 2. API reference

| Endpoint | Method | Body | Query | Returns |
|---|---|---|---|---|
| `/compile/book` | POST | EPUB or KIR bytes | `format` (`auto`\|`epub`\|`kir`), `theme` (YAML) | `.koma` bytes |
| `/render/document` | POST | `.koma` bytes | `chapter`, `width`, `height`, `backend` (`software`\|`gpu`), `theme` (YAML) | PNG bytes |
| `/session/open` | POST | `.koma` bytes | — | JSON `{ session_id, title, chapters:[{id,title}] }` |
| `/scene/state` | GET | — | `session`, `chapter` | Scene JSON |

Errors return the HTTP status plus `{"error": "<message>"}`.

## 3. Dart client

Add `http` to `pubspec.yaml`, then copy the `KomaClient` class from
[flutter-integration.md](flutter-integration.md) unchanged — it talks only to
the network service and is parser-agnostic.

## 4. Wiring into an epub_pro reader

epub_pro reads the whole book with `EpubReader.readBook(bytes)` and exposes
title, authors, nested chapters, HTML content, and the raw OPF/NCX schema:

```dart
import 'dart:typed_data';
import 'package:epub_pro/epub_pro.dart';

// 1. Load the epub bytes (your epub_pro app already has these).
final Uint8List epubBytes = await _loadBookBytes(); // File(...).readAsBytes() / rootBundle

// 2. epub_pro = content truth: title, chapters, HTML, schema.
final EpubBook book = await EpubReader.readBook(epubBytes);
final String? title = book.title;
for (final EpubChapter chapter in book.chapters) {
  // chapter.title, chapter.htmlContent, chapter.subChapters
}

// 3. KRE = presentation truth: compile the same bytes, then open a session.
final KomaClient koma = KomaClient('http://127.0.0.1:7878');
final Uint8List komaBytes = await koma.compileBook(epubBytes);
final session = await koma.openSession(komaBytes);
final String sessionId = session['session_id'] as String;

// 4. Render chapter ids come from the KRE package (spine order).
final List chapters = session['chapters'] as List;
final String firstChapter = (chapters.first as Map)['id'] as String;

final Uint8List png = await koma.renderDocument(
  komaBytes,
  chapter: firstChapter,
  width: 900,
  height: 1200,
);
```

Display with `Image.memory(png)` inside an `InteractiveViewer`, and page
chapters with a small LRU as in the epubx guide.

### epub_pro specifics to know

- **TOC vs render ids.** epub_pro's `book.chapters` are nested and carry the
  hierarchy for your navigation UI. KRE compiles the flat spine, so chapter
  ids for `/render/document` and `/scene/state` come from
  `/session/open` → `chapters`. Keep the two indexes separate: epub_pro for
  TOC/text/selection, the KRE session for rendering/scenes.
- **Lazy loading.** `EpubReader.openBook(bytes)` returns an `EpubBookRef`
  (metadata only) — good for the app shell. KRE compiles from the full bytes
  when you're ready to render.
- **Split chapters.** `readBookWithSplitChapters()` / `openBookWithSplitChapters()`
  split long chapters (>3000 words) for your text UI. KRE always compiles the
  original spine; if you split client-side, recompile with the unmodified
  bytes so render ids stay stable.
- **Schema access.** `book.schema?.package` (OPF) and `book.schema?.navigation`
  (NCX) give you the raw metadata for a library/bookshelf screen while KRE
  handles the reading experience.

## 5. Scene state and atmosphere

Same as the epubx guide — `GET /scene/state` returns the chapter's compiled
scene (environment, background, effects, timeline) for your chrome:

```dart
final scene = await koma.sceneState(sessionId, chapter: id);
final background = scene['environment']['background'] as String;
```

## 6. Notes and limits

- **Sessions are in-memory.** Re-open when the app restarts; cache the
  compiled `.koma` bytes in app storage to skip recompilation.
- **Theme override.** Pass a YAML theme string in the `theme` query param to
  override the package scene; omit it to use the compiled scene.
- **Backend.** `backend=gpu` uses the wgpu renderer when available, falling
  back to software.
- **Text & accessibility.** KRE frames are raster PNGs; keep epub_pro for
  text selection and screen readers. The scene JSON carries the semantic
  structure for navigation.
- **Privacy.** Content stays on-device and on localhost; the engine never
  transmits books, history, or preferences.
- **Embedding.** Want KRE in-process instead of over HTTP? See
  [Embedding via FFI](flutter-integration.md#embedding-via-ffi) in the epubx
  guide — the client code is identical either way.