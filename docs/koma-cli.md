# koma-cli — Developer Toolchain Reference

`koma` is the KRE command-line toolchain. It mirrors the compiler, renderer,
and network service operations as a local binary.

```bash
cargo build -p koma-cli
./target/debug/koma --help
```

## Commands

| Command | Purpose |
|---|---|
| `koma import` | EPUB → KIR (`.kir`) |
| `koma compile` | KIR or EPUB → `.koma` package |
| `koma inspect` | package manifest + chapter/asset/theme/scene index |
| `koma validate` | decode every chapter, report errors |
| `koma preview` | plain-text chapter preview |
| `koma render` | render a chapter to PNG (software or GPU) |
| `koma serve` | run the HTTP network service |

### `koma import`

```
koma import book.epub [--out out.kir]
```

Converts an EPUB into a KIR document via the `koma-epub` adapter. Default
output is `<input>.kir`. Unsupported extensions error out.

### `koma compile`

```
koma compile book.kir|book.epub [--out book.koma] [--assets DIR] [--theme theme.yaml]
```

Compiles a KIR document (or an EPUB, imported inline) into a `.koma`
package. Default output is `<input>.koma`.

- `--assets DIR` — directory of asset files keyed by media id; files are
  read into the package.
- `--theme theme.yaml` — theme YAML file (`theme-spec-v0.1.md`) embedded in
  the package; used to synthesize per-chapter scenes.

Prints `compiled N chapter(s), M asset(s)[, theme "name"] -> out.koma`.

### `koma inspect`

```
koma inspect book.koma
```

Prints the package manifest:

```
format:     koma
version:    0.1.0
generator:  koma-compiler 0.1.0
seed:       <u64>
document:   <title>
author:     <author>
language:   <lang>
chapters (N):
  <id>  <title>  (<bytes> bytes)
assets (N):
  <id>  <bytes> bytes
theme:      <path or (none)>
scenes (N):
  <id>  <bytes> bytes
```

### `koma validate`

```
koma validate book.koma
```

Checks format version and decodes every chapter. Prints
`valid: N chapter(s) decode correctly` or exits with status 1 after printing
`invalid package: <error>`.

### `koma preview`

```
koma preview book.koma [--chapter ch1]
```

Prints the plain text of a chapter (default: first chapter). Text-extraction
/ accessibility path, independent of rendering.

### `koma render`

```
koma render book.koma [--chapter ch1] [--out chapter.png]
                     [--width 900] [--height 1200]
                     [--theme theme.yaml] [--backend software|gpu]
```

Renders a chapter to a PNG frame.

- `--chapter` — chapter id (default: first chapter).
- `--out` — output PNG path (default `chapter.png`).
- `--width` / `--height` — frame size in pixels (defaults 900×1200).
- `--theme` — theme YAML override. An explicit override wins over the
  package scene; without it, the chapter's compiled scene (environment
  background + text-layer typography) is applied.
- `--backend` — `software` (default) or `gpu`. `gpu` uses the wgpu
  renderer; when no GPU adapter is available it prints
  `gpu backend unavailable (...); falling back to software` and renders
  with the software backend (AGENTS.md failure handling).

Prints `rendered chapter "id" (WxH)[ with theme "name" | in <env> environment] via <backend> -> out.png`.

### `koma serve`

```
koma serve [--addr 127.0.0.1:7878]
```

Runs the HTTP network service (Phase 8) on the given bind address. Endpoints
and request/response formats: [`network-api-spec-v0.1.md`](network-api-spec-v0.1.md).
Prints `koma serve listening on http://<addr>` and serves until stopped.

## Notes

- Exit status is non-zero on any error; errors print to stderr.
- GPU and software backends are content-correct but not bit-identical.
- Compilation is deterministic for a given source, compiler version, and
  theme (AGENTS.md Reproducibility).
- The same operations are available over HTTP via `koma serve` for
  language-independent integration.