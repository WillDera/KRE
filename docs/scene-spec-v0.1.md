# Scene Graph Format Specification — v0.1

**Format version:** `0.1.0` (`SCENE_VERSION`)

A scene is the compiled presentation of one chapter: environment, lighting,
a layer graph, transitions, and a timeline. Scenes are **presentation
truth** — they never modify content. They are produced by the compiler
(per-chapter, deterministic) and embedded in `.koma` packages under
`scenes/{chapter_id}.json`; the runtime reads them and never synthesizes
them. The scene system is independent of any concrete rendering backend
(AGENTS.md: the scene system must not directly depend on wgpu).

Scenes serialize to JSON (`Scene::to_json()` / `Scene::parse_json()`).

## Format

```json
{
  "version": "0.1.0",
  "chapter_id": "ch1",
  "environment": {
    "kind": "abstract",
    "background": "#0b0e14",
    "atmosphere": { "frost": 0.6 }
  },
  "lighting": {
    "ambient": { "color": "#e8e6e3", "intensity": 0.85 },
    "directional": {
      "color": "#ffffff",
      "intensity": 0.4,
      "direction": [0.0, -1.0, 0.5]
    }
  },
  "root": {
    "id": "root",
    "kind": "root",
    "children": [
      {
        "id": "text",
        "kind": "text",
        "children": [],
        "effects": [],
        "typography": { "font_family": "Cormorant", "font_size": 22.0 },
        "animation": "open"
      }
    ],
    "effects": [],
    "animation": null
  },
  "transitions": [
    { "from": null, "to": "ch1", "kind": "fade", "duration": 0.8 }
  ],
  "timeline": {
    "duration": 5.0,
    "events": [
      { "time": 0.0, "action": { "kind": "set_background", "target": "root", "params": {} } },
      { "time": 2.0, "action": { "kind": "set_frost", "target": null, "params": { "intensity": 0.5 } } }
    ]
  }
}
```

## Types

### `Scene`

| Field | Type | Notes |
|---|---|---|
| `version` | string | REQUIRED; must equal `"0.1.0"` |
| `chapter_id` | string | REQUIRED; non-empty; chapter this scene presents |
| `environment` | `Environment` | |
| `lighting` | `Lighting` | |
| `root` | `Node` | scene-graph root |
| `transitions` | array of `Transition` | default `[]` |
| `timeline` | `Timeline` | |

### `Environment`

| Field | Type | Notes |
|---|---|---|
| `kind` | enum | `indoor` \| `outdoor` \| `space` \| `abstract` |
| `background` | color | hex, see theme spec |
| `atmosphere` | map[string, number] | declarative (e.g. `frost`, `haze`); default `{}` |

### `Lighting`

| Field | Type | Notes |
|---|---|---|
| `ambient` | `LightSource` | `{ color, intensity }`; intensity default `0.8` (0.0 dark – 1.0 full) |
| `directional` | `DirectionalLight` (optional) | `{ color, intensity, direction: [x, y, z] }`; direction is normalized |

### `Node`

| Field | Type | Notes |
|---|---|---|
| `id` | string | |
| `kind` | enum | `root` \| `background` \| `text` \| `effects` \| `image` |
| `children` | array of `Node` | compose under their parent; default `[]` |
| `effects` | array of `Effect` | effects attached to this layer; default `[]` |
| `typography` | `Typography` (optional) | presentation-only override |
| `animation` | string (optional) | timeline animation this node participates in |

### `Effect`

| Field | Type | Notes |
|---|---|---|
| `id` | string | |
| `kind` | enum | `snow` \| `rain` \| `fog` \| `dust` \| `stars` \| `light_rays` \| `particles` |
| `enabled` | bool | default `false` |
| `params` | map[string, string] | free-form (`density`, `speed`, `wind`, ...); default `{}` |

### `Transition`

| Field | Type | Notes |
|---|---|---|
| `from` | string (optional) | `null` when this is the entry transition into the document |
| `to` | string | |
| `kind` | enum | `fade` \| `slide` \| `none` |
| `duration` | number | seconds; must be `>= 0` |

### `Timeline`

| Field | Type | Notes |
|---|---|---|
| `duration` | number | total seconds; must be `>= 0` |
| `events` | array of `TimelineEvent` | default `[]` |

### `TimelineEvent` / `TimelineAction`

`TimelineEvent` = `{ time, action }` where `time` is seconds from timeline
start (must be `>= 0`) and `action` is:

| Field | Type | Notes |
|---|---|---|
| `kind` | enum | `set_background` \| `set_lighting` \| `set_frost` \| `reveal` \| `fade_in` \| `fade_out` \| `start_effect` \| `stop_effect` \| `play_ambient` |
| `target` | string (optional) | target node/effect id; `null` means the whole scene |
| `params` | map[string, number] | numeric params (opacity, intensity, ...); default `{}` |

`play_ambient` is an audio trigger and is deferred — the engine must function
without audio (AGENTS.md Audio Engine).

## Validation

`Scene::validate()` enforces:

- `version == "0.1.0"` (else `UnsupportedVersion`)
- `chapter_id` non-empty
- `lighting.ambient.intensity >= 0`
- `lighting.directional.intensity >= 0` (when present)
- `timeline.duration >= 0`
- every `timeline.events[].time >= 0`
- every `transitions[].duration >= 0`

## Determinism

Scene synthesis (`default_scene_for_chapter(chapter_id, theme, blocks)`) is a
compiler responsibility and is deterministic: hash-map collection keys are
sorted before iteration (e.g. theme effect/particle ids), so identical inputs
produce identical scenes. A compiled `.koma` is reproducible (AGENTS.md
Reproducibility).

Default synthesis produces: environment background from the theme palette,
ambient lighting from the theme text color, a text layer carrying the theme
typography, effect layers for enabled theme effects/particles, an opening
fade-in timeline, and an entry transition.

## Embedding in `.koma`

The compiler embeds one scene per chapter:

- manifest: `scenes: [ { id, path, bytes } ]`
- path: `scenes/{chapter_id}.json`
- runtime access: `KomaPackage::scene(&mut self, id)` → `Scene`

The runtime never performs scene generation; it only loads embedded scenes.

## Compatibility

This format is versioned; breaking changes require a version bump and
migration (AGENTS.md Compatibility). The timeline is independent of
rendering, and the scene system has no dependency on any concrete GPU
backend.

## See also

- Theme format: `theme-spec-v0.1.md` (scenes carry theme typography/effects)
- `.koma` package format: `koma-format-spec-v0.1.md`
- Renderer mapping: `layout_config_from_scene` applies environment background
  and text-layer typography to the layout config