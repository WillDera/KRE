# Theme Format Specification — v0.1

**Format version:** `0.1.0` (`THEME_VERSION`)

Themes are **presentation truth** (AGENTS.md): they never alter content or
semantics. They are YAML-serializable, user-editable, shareable, and
versioned. A theme is a standalone artifact — it can be embedded in a `.koma`
package by the compiler or passed to the renderer directly (CLI `--theme`,
network service `?theme=`).

Current renderer scope: only `typography` and `colors` take effect in the
software renderer. `effects` and `particles` are declarative and versioned
for forward compatibility; the renderer stores and validates them but renders
them inert (static fallback per AGENTS.md failure handling). Audio is
deferred.

## Format

```yaml
version: "0.1.0"     # REQUIRED; must match THEME_VERSION
name: Imperial Archive   # REQUIRED; non-empty
info:                # optional; asset attribution
  name: ...          # optional
  creator: ...       # optional
  license: ...       # optional
  source: ...        # optional
  version: ...       # optional
typography:
  font_family: Cormorant   # optional; renderer falls back when not installed
  font_size: 22            # default 20.0  (device px; must be > 0)
  line_height: 28          # default 28.0  (must be > 0)
  paragraph_spacing: 12    # default 12.0  (must be >= 0)
  margin: 48               # default 48.0  (must be >= 0)
colors:
  background: "#0b0e14"    # default #ffffff
  text: "#e8e6e3"          # default #000000
  heading: "#1a1a2e"       # default #1a1a2e
  quote: "#444444"         # default #444444
effects:                 # map: effect id -> effect
  frost:
    enabled: true
    params:
      strength: "0.6"   # free-form string params
particles:               # map: particle id -> particle
  snow:
    density: medium     # optional free-form
    speed: slow         # optional free-form
    wind: north         # optional free-form
    params: {}          # additional free-form params
```

Missing sections default to the values above; the only required fields are
`version` and `name`.

## Types

### Colors

Colors use CSS-style hex strings, parsed case-insensitively:

- `#RGB` (e.g. `#f00`)
- `#RRGGBB` (e.g. `#0b0e14`)
- `#RRGGBBAA` (e.g. `#0b0e14ff`)

### `Typography`

| Field | Type | Default | Constraints |
|---|---|---|---|
| `font_family` | string (optional) | — | renderer falls back to its default when absent/not installed |
| `font_size` | number | `20.0` | `> 0` |
| `line_height` | number | `28.0` | `> 0` |
| `paragraph_spacing` | number | `12.0` | `>= 0` |
| `margin` | number | `48.0` | `>= 0` |

### `ColorScheme`

| Field | Type | Default |
|---|---|---|
| `background` | color | `#ffffff` |
| `text` | color | `#000000` |
| `heading` | color | `#1a1a2e` |
| `quote` | color | `#444444` |

### `Effect`

Declarative render effect (frost, light rays, ...).

| Field | Type | Default |
|---|---|---|
| `enabled` | bool | `false` |
| `params` | map[string, string] | `{}` |

### `Particle`

Declarative particle effect (snow, rain, stars, ...). Free-form values keep
the schema open while scenes mature (e.g. `density: 0.5`, `speed: slow`,
`wind: north`).

| Field | Type | Default |
|---|---|---|
| `density` | string (optional) | — |
| `speed` | string (optional) | — |
| `wind` | string (optional) | — |
| `params` | map[string, string] | `{}` |

### `ThemeInfo`

Attribution block (AGENTS.md License and Attribution System). All fields
optional strings: `name`, `creator`, `license`, `source`, `version`.

## Validation

`Theme::validate()` enforces:

- `version == "0.1.0"` (else `UnsupportedVersion`)
- `name` is non-empty
- `typography.font_size > 0`
- `typography.line_height > 0`
- `typography.paragraph_spacing >= 0`
- `typography.margin >= 0`

## Serialization

Themes serialize to YAML via `Theme::parse_yaml(bytes)` / `Theme::to_yaml()`.
Field order is stable (struct definition order), so serialization round-trips
byte-for-byte-comparable output.

## Canonical example

```yaml
version: "0.1.0"
name: Imperial Archive
info:
  creator: Koma Studio
  license: MIT
typography:
  font_family: Cormorant
  font_size: 22
colors:
  background: "#0b0e14"
  text: "#e8e6e3"
effects:
  frost:
    enabled: true
    params:
      strength: "0.6"
particles:
  snow:
    density: medium
    speed: slow
    wind: north
```

## Compatibility

This format is versioned. Breaking changes require a version bump and a
migration path (AGENTS.md Compatibility). A minimal theme — valid for
programmatic use when no author theme exists — is `Theme::minimal(name)`
(all defaults, empty effects/particles).

## See also

- Scene graph format: `scene-spec-v0.1.md` (scenes carry typography and
  effects into the compiled package)
- `.koma` package format: `koma-format-spec-v0.1.md` (themes are embedded by
  the compiler and readable from the runtime)