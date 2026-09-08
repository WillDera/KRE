# Theme Format Specification — v0.2

**Format version:** `0.2.0` (`THEME_VERSION`)

v0.2 adds a **genre selection axis** and **per-role typography** (drop caps,
first-line indent, justification, weight, decorative rules). Themes remain
**presentation truth** (AGENTS.md): they never alter content or semantics.

**Migration:** `0.1.0` themes remain valid (`SUPPORTED_THEME_VERSIONS`).
Missing `genre` / `roles` behave as before (engine defaults for heading scale).

## Format

```yaml
version: "0.2.0"          # REQUIRED; 0.1.0 also accepted
name: Imperial Archive    # REQUIRED; non-empty
genre: fantasy            # optional: literary | fantasy | technical | noir | …
roles:                    # optional overrides (keys: body, h1|heading, h2, h3, quote)
  body:
    first_line_indent: 28
    justify: true
    drop_cap:
      enabled: true
      lines: 3
      scale: 3.0
  h1:
    weight: bold
    scale: 1.6
    decorative_rule:
      enabled: true
      thickness: 1.0
      gap: 10.0
typography:
  font_family: Cormorant
  font_size: 22
  line_height: 28
  paragraph_spacing: 12
  margin: 48
colors:
  background: "#0b0e14"
  text: "#e8e6e3"
  heading: "#cfc4ff"
  quote: "#aab4c8"
effects: {}
particles: {}
```

## Genre axis

When `genre` is set, built-in presets supply role defaults. Explicit `roles`
entries override the preset field-by-field (theme wins on set values).

| Genre | Body | Headings |
|---|---|---|
| `literary` | indent + justify + drop cap | bold + scale + decorative rule |
| `fantasy` | indent + drop cap | bold + larger scale + rule |
| `technical` / `textbook` | no indent, no drop cap | bold + moderate scale |
| `noir` | indent + justify | bold + thick rule |
| other / unset | engine defaults | hardcoded heading scale (1.5 / 1.25) |

## Role fields

| Field | Applies to | Effect |
|---|---|---|
| `first_line_indent` | body / quote | shifts first-line glyphs |
| `justify` | any | cosmic-text `Align::Justified` |
| `weight` | any | `normal` \| `bold` |
| `scale` | headings | multiplier of body `font_size` |
| `drop_cap` | body paragraphs | oversized first character + hang indent |
| `decorative_rule` | headings | horizontal rule under the heading block |

## Validation

In addition to v0.1 checks:

- `version` ∈ `{0.1.0, 0.2.0}`
- `roles.*.scale > 0` when set
- `roles.*.drop_cap.scale > 0` when enabled
- `roles.*.decorative_rule.thickness >= 0` when enabled
- `roles.*.first_line_indent >= 0`

## See also

- v0.1 baseline: `theme-spec-v0.1.md`
- Layout consumes resolved roles via `Theme::resolved_roles()` → `LayoutConfig.roles`
