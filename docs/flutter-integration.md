# Integrating KRE with Flutter (epubx)

Koma Rendering Engine (KRE) is a Rust engine; Flutter apps are Dart. The
integration point designed for external software is the **Koma network
service** (AGENTS.md: Integration API) — a language-independent HTTP API
exposed by `koma serve` (Phase 8). A Flutter app talks to it over plain
HTTP with `package:http`; no FFI, no Rust toolchain in the app build.

This guide walks through wiring KRE into an existing Flutter ebook reader
that already uses [epubx](https://pub.dev/packages/epubx) to parse EPUBs.

```
Flutter app (epubx parses EPUB)
      │  EPUB bytes / .koma bytes
      ▼
 koma serve  (POST /compile/book, POST /render/document,
              POST /session/open, GET /scene/state)
      │
      ▼
 .koma package · PNG frames · scene JSON
```

## Why HTTP (and when to consider FFI)

- **Recommended: network service.** Works today, language-independent, and
  KRE's `ContentSource` / session model is built for it. The app stays a
  normal Dart/Flutter app.
- **Embedded (future): `flutter_rust_bridge`.** Compile KRE into the app as
  a Rust library and call it over FFI. Tighter integration (no localhost
  process), but needs a Rust toolchain and per-platform build setup, plus a
  thin Rust FFI crate. See [Embedding via FFI](#embedding-via-ffi) below.

## 1. Run the service

```bash
cargo run -p koma-cli -- serve --addr 127.0.0.1:7878
```

or build once and run the binary:

```bash
cargo build -p koma-cli
./target/debug/koma serve --addr 127.0.0.1:7878
```

The service binds `127.0.0.1:7878` by default. Do **not** expose it on a
public interface — uploaded content and sessions are private.

## 2. API reference

| Endpoint | Method | Body | Query | Returns |
|---|---|---|---|---|
| `/compile/book` | POST | EPUB or KIR bytes | `format` (`auto`\|`epub`\|`kir`), `theme` (YAML) | `.koma` bytes |
| `/render/document` | POST | `.koma` bytes | `chapter`, `width`, `height`, `backend` (`software`\|`gpu`), `theme` (YAML) | PNG bytes |
| `/session/open` | POST | `.koma` bytes | — | JSON `{ session_id, title, chapters:[{id,title}] }` |
| `/scene/state` | GET | — | `session`, `chapter` | Scene JSON |

Errors return the matching HTTP status with a JSON body:
`{"error": "<message>"}`.

## 3. Dart client

`pubspec.yaml`:

```yaml
dependencies:
  http: ^1.2.0
```

```dart
import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

/// Thin client for the Koma network service.
class KomaClient {
  KomaClient(this.baseUrl); // e.g. 'http://127.0.0.1:7878'

  final String baseUrl;

  static const _octetStream = {'content-type': 'application/octet-stream'};

  /// EPUB/KIR bytes -> compiled `.koma` package bytes.
  Future<Uint8List> compileBook(
    Uint8List source, {
    String format = 'auto',
    String? themeYaml,
  }) async {
    final uri = Uri.parse('$baseUrl/compile/book').replace(queryParameters: {
      if (format.isNotEmpty) 'format': format,
      if (themeYaml != null) 'theme': themeYaml,
    });
    return _bodyBytes(await http.post(uri, body: source, headers: _octetStream));
  }

  /// `.koma` bytes -> a PNG frame of one chapter.
  Future<Uint8List> renderDocument(
    Uint8List koma, {
    String? chapter,
    int width = 900,
    int height = 1200,
    String backend = 'software',
  }) async {
    final uri = Uri.parse('$baseUrl/render/document').replace(queryParameters: {
      if (chapter != null) 'chapter': chapter,
      'width': '$width',
      'height': '$height',
      'backend': backend,
    });
    return _bodyBytes(await http.post(uri, body: koma, headers: _octetStream));
  }

  /// `.koma` bytes -> an in-memory session id + chapter index.
  Future<Map<String, dynamic>> openSession(Uint8List koma) async {
    final res = await http.post(
      Uri.parse('$baseUrl/session/open'),
      body: koma,
      headers: _octetStream,
    );
    return _json(res);
  }

  /// Scene JSON for a chapter (environment, background, effects, timeline).
  Future<Map<String, dynamic>> sceneState(
    String sessionId, {
    String? chapter,
  }) async {
    final uri = Uri.parse('$baseUrl/scene/state').replace(queryParameters: {
      'session': sessionId,
      if (chapter != null) 'chapter': chapter,
    });
    return _json(await http.get(uri));
  }

  Uint8List _bodyBytes(http.Response res) {
    if (res.statusCode < 200 || res.statusCode >= 300) {
      throw KomaException(res.statusCode, _errorMessage(res));
    }
    return res.bodyBytes;
  }

  Map<String, dynamic> _json(http.Response res) {
    if (res.statusCode < 200 || res.statusCode >= 300) {
      throw KomaException(res.statusCode, _errorMessage(res));
    }
    return jsonDecode(utf8.decode(res.bodyBytes)) as Map<String, dynamic>;
  }

  String _errorMessage(http.Response res) {
    try {
      return (jsonDecode(utf8.decode(res.bodyBytes)) as Map<String, dynamic>)
          ['error'] as String? ?? 'HTTP ${res.statusCode}';
    } catch (_) {
      return 'HTTP ${res.statusCode}';
    }
  }
}

class KomaException implements Exception {
  KomaException(this.statusCode, this.message);
  final int statusCode;
  final String message;
  @override
  String toString() => 'KomaException($statusCode): $message';
}
```

## 4. Wiring into an epubx reader

epubx already gives you the content: `EpubReader.readBook(bytes)` yields the
book metadata, spine order, and parsed chapters. KRE takes the **same bytes**
and compiles them into its own package. Two parallel truths (AGENTS.md):

- **Content truth** stays in the EPUB / epubx objects: TOC, text, selection,
  and the accessibility/text path.
- **Presentation truth** lives in KRE: scenes, environments, effects, and
  GPU-rendered frames.

```dart
// 1. Load the epub (existing epubx app already does this).
final ByteData data = await rootBundle.load('assets/book.epub');
final Uint8List epubBytes = data.buffer.asUint8List();

// 2. Parse with epubx (content truth — for TOC, text, selection).
final EpubBook book = await EpubReader.readBook(bytes);
final String title = book.Title;

// 3. Compile to KRE and open a session (presentation truth).
final KomaClient koma = KomaClient('http://127.0.0.1:7878');
final Uint8List komaBytes = await koma.compileBook(epubBytes);
final session = await koma.openSession(komaBytes);
final String sessionId = session['session_id'] as String;

// 4. Chapter ids come from the KRE package (spine order), not epubx.
final List chapters = session['chapters'] as List;
final String firstChapter = (chapters.first as Map)['id'] as String;

// 5. Render the first chapter page.
final Uint8List png = await koma.renderDocument(
  komaBytes,
  chapter: firstChapter,
  width: 900,
  height: 1200,
);
```

### A minimal reader widget

```dart
class KomaPageView extends StatelessWidget {
  const KomaPageView({super.key, required this.pngBytes});

  final Uint8List pngBytes;

  @override
  Widget build(BuildContext context) {
    return InteractiveViewer(
      minScale: 0.5,
      maxScale: 4.0,
      child: Image.memory(pngBytes, fit: BoxFit.contain),
    );
  }
}
```

## 5. Navigating chapters

Request one chapter at a time and keep a small LRU of decoded pages —
mirrors KRE's lazy-loading / memory-eviction model. The `chapters` array from
`/session/open` is the navigation index.

```dart
final Map<String, Uint8List> _pages = {};
final List<String> _recent = []; // naive LRU order

Future<void> _showChapter(String id) async {
  if (!_pages.containsKey(id)) {
    final png = await koma.renderDocument(
      _komaBytes,
      chapter: id,
      width: 900,
      height: 1200,
      backend: 'gpu', // falls back to software when unavailable
    );
    _pages[id] = png;
    _recent.add(id);
    if (_recent.length > 6) {
      final evicted = _recent.removeAt(0);
      _pages.remove(evicted); // memory eviction
    }
  }
  setState(() => _current = id);
}
```

## 6. Scene state and atmosphere

`GET /scene/state` returns the compiled scene for a chapter: environment
kind and background, typography, effects, transitions, and the timeline.
Use it to mirror the chapter's atmosphere in your own chrome (e.g., page
background color, title styling) before the render arrives:

```dart
final scene = await koma.sceneState(sessionId, chapter: id);
final background = scene['environment']['background'] as String; // '#0b0e14'
final effects = scene['root']['children']; // effect nodes
```

## 7. Notes and limits

- **Sessions are in-memory.** `koma serve` restart clears them; `scene/state`
  requires a session id from this process lifetime. Re-open when the app
  restarts.
- **Cache the `.koma`.** Compilation is one-time per book; store `komaBytes`
  in the app documents dir and only recompile when the source changes.
- **Theme override.** Pass a theme as a YAML string in the `theme` query
  param to override the package-embedded scene; omit it to use the compiled
  scene (CLI rule: an explicit override wins over the scene).
- **Backend.** `backend=gpu` uses the wgpu/Metal renderer when available and
  falls back to software (AGENTS.md failure handling). `software` is
  deterministic and safest for tests.
- **Text & accessibility.** KRE frames are raster PNGs. Keep epubx (or the
  `.koma` chapter text) as the source for text selection and screen readers.
  The scene JSON exposes semantic structure for navigation and accessibility.
- **Privacy.** Content stays on the device and the localhost service; the
  engine never transmits books, reading history, or preferences.

## Embedding via FFI

Future option for apps that want the engine in-process:

1. Add a small FFI crate that wraps `KomaCompiler::compile_with_theme`,
   `SoftwareBackend` / `WgpuBackend::render_blocks`, and `KomaPackage::open`,
   exposing bytes-in/bytes-out functions.
2. Bind it with `flutter_rust_bridge`; run the renderer off the UI thread and
   ship PNG bytes to Flutter as in the HTTP client above.
3. Watch for 100MB-book memory behavior on mobile; the software backend keeps
   peak usage low and works on all platforms.

The HTTP service remains the recommended path — the FFI crate would just
remove the localhost hop. KRE's public interfaces (KIR, `.koma`, plugin API,
network API) are versioned so either path can be swapped without changing
your app's data flow.