# opurom-render

Independent Rust crate: IR v0.1 → adaptive layout → deterministic RGBA8 software
raster → incremental dirty rects, plus a deterministic SVG 1.1 **vector software
backend**, deterministic animation primitives, an exact display-list export and
five deterministic framebuffer effects, plus optional effect commands that
reproduce them on the cross-repo Metal compute backend. This folder is its own
git repository; it has no source dependency on
any other OPUROM layer, only on the frozen IR/ABI contracts
(`IR_SCHEMA.md`, `ADAPTER_CONTRACT.md`) and local `FrameView` shape
compatibility.

## Verify

```sh
cd opurom-render
cargo fmt --all
cargo fmt --check
cargo test --offline
```

`cargo test --offline` runs 185 tests (unit + integration). All pass, zero
warnings. Only `serde_json` is used; it is pinned to the locally cached
`=1.0.154`, so the build needs no network.

CLI (integration surface for `opurom-cli` and the e2e suite):

```sh
cargo build --offline
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm
# digest=c8403ba57da5ee61
# dirty=1
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --state count=7
# digest=48a56056bd32a07e
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --a11y tree.json
# digest=c8403ba57da5ee61, dirty=1 (stdout unchanged)
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --svg out.svg
# writes a deterministic SVG 1.1 snapshot (stdout unchanged)
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 320 240 out.ppm --commands commands.json
# writes the exact display list (stdout unchanged)
./target/debug/opurom-render tests/fixtures/effects.opuir.json 96 64 fx.ppm --commands fx.commands.json
# writes the display list plus effect commands (stdout unchanged)
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --transition slide --tick 16
# deterministic transition digest (stdout still exactly digest=... and dirty=...)
```

Usage: `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick 0..4000] [--transition slide|fade|scale] [--a11y out.json] [--svg out.svg] [--commands out.json]`.
`--state` repeats; values are parsed against the declared state type. `--tick`
sets the animation clock position in milliseconds, accepts `0..=4000`, and
rejects non-numeric or out-of-range values with a readable diagnostic; the
default is the settled end state, so a v0.1 IR renders exactly as the core release.
`--transition` is opt-in: the CLI uses the frame at tick 0 as the outgoing page
and the settled frame as the incoming page, then uses `--tick` as the elapsed
time of the 320 ms transition. Unknown transition kinds are rejected with exit
code 1.
Output stays a P6 PPM (RGB; the opaque alpha byte is dropped). Stdout has
exactly two lines: `digest=<16 lowercase hex>` (FNV-1a 64 over the **RGBA**
frame bytes) and `dirty=<rect count>`. Malformed IR or a bad viewport is a
readable stderr diagnostic with exit code 1, never a panic. A fresh process
always reports `dirty=1` (first frame = whole surface); unchanged state within
one `Renderer` yields zero dirty rects. `--a11y <out.json>` writes a separate
deterministic JSON accessibility tree (see below), and `--svg <out.svg>` a
deterministic SVG 1.1 snapshot; neither adds stdout lines and neither is
composited by `--transition`. `--commands <out.json>` records the exact raster
display list as deterministic JSON: solid rects, rounded rects, two-stop
vertical gradients and particle fields with integer bounds/colours in draw order, plus the viewport
and opaque background. Bitmap glyph runs are decomposed on the CPU into solid
rect commands; there is no GPU glyph atlas or GPU text shaping. CPU-only page
transitions cannot be represented as this display list, so `--commands` and
`--transition` together fail with a readable diagnostic. The display list adds
optional effect commands for IR carrying visual effects: `effect_backdrop` and
`effect_mask` snapshot the frame and the node silhouette, `effect_shadow`,
`effect_glow`, `effect_glass` and `effect_liquid` carry the clamped integer
parameters (radius, dx/dy, fixed-point alpha, colour, bounds, shape radius),
and `effect_blur` is a whole-frame pass appended after all nodes. Effect-free
IR emits none of them, so the frozen effect-free command fixture stays byte-identical.
The flag adds no stdout lines and does not change the PPM or golden digests.

## Architecture

```
serde_json::Value ── parse_ir (validate) ──▶ Ir { state, tree: Node, theme }
                                               │
                           StateView (flat BTreeMap, {name} interpolation)
                                               │
                      layout(Ir, StateView, viewport) ──▶ Vec<LaidOut>  (pre-order)
                                               │
                               ├──▶ raster::Canvas (rounded fills/borders + 5x7 glyphs + local effects) ──▶ final frame blurs ──▶ Vec<u8> RGBA8
                               │                       │
                               │                       ├──▶ Vec<Command> ──▶ DisplayList JSON
                                │                       │
                                │       Renderer::render diff vs previous buffer ──▶ Frame { dirty }
                               │                       │
                               │               Frame::view() ──▶ FrameView<'_> for adapters
                               │
                               └──▶ svg::svg_at (same Vec<LaidOut>) ──▶ deterministic SVG 1.1 bytes
```

- `src/ir.rs` — strictly validates `ir == "0.1"`, state decls/types/inits, pre-order
  node ids, the 100 node kinds (32 core kinds + 68 extended kinds), known props, event actions
  (targets/types) and the
  theme. Unknown props are ignored. Errors are `IrError(String)` values.
- `src/layout.rs` — pure integer layout. Breakpoints on **logical px**: compact
  `<600`, medium `<900`, expanded `<1200`, wide `<1600`, ultra `>=1600`. `Row`
  collapses to a vertical stack in compact. Children share the main axis in
  proportion to their natural size (fluid
  sizing, i128 division with a deterministic remainder), honouring explicit
  `width`/`height`. `gap`, `padding`, `margin`, `align`, `radius` and `color` are
  resolved here.
- `src/font.rs` — built-in 5x7 bitmap font, printable ASCII `0x20..=0x7E` (A–Z,
  a–z, 0–9, punctuation). Unsupported codepoints consume a blank advance.
- `src/raster.rs` — filled rounded rectangles (integer quarter-circle test), 1 px
  borders, gradients, shadows, glass fills, supersampled anti-aliased glyphs,
  opaque RGBA with integer alpha blending, the eight data primitive drawings and
  exact command recording. Text coverage is 4x4 bilinear supersampling of the
  5x7 pattern with a ~1 px feather; advance metrics, line wrapping and SVG
  metrics are unchanged. The rasterizer operates on the built-in bitmap pattern with deterministic integer coverage.
- `src/effects.rs` — integer RGBA framebuffer effects: separable box blur,
  silhouette shadow/glow, frosted backdrop glass and a specular liquid gradient.
- `src/particles.rs` — the opt-in particle field: bounded integer parameters,
  the seeded integer dot-centre hash and the fixed-point alpha reduction.
- `src/display.rs` — fixed-shape deterministic display-list JSON with integer
  bounds/colours, draw order, viewport and opaque background.
- `src/anim.rs` — the fixed-timestep animation clock, spring/ease/inertia curves, page
  transitions, gesture progress, fixed-seed particles with wrap and bounce
  collision modes, and quality tiers.
- `src/lib.rs` — `Rgba`, `StateView`, `Renderer`, `Frame`, `FrameView`, inline FNV-1a
  digest, and row/column dirty-rect diffing.
- `src/a11y.rs` — pre-order accessibility tree, its deterministic JSON serializer,
  and the shared `role_of` helper used by the SVG `data-role` backend.
- `src/svg.rs` — deterministic SVG 1.1 vector snapshot of the same layout.
- `src/main.rs` — the CLI described above.

## Interactive UI features (IR_ADDITIONS)

All interactive UI fields are **additive and opt-in**. A v0.1 IR file renders
byte-identically to the core release — the three golden digests below are unchanged.

| Area | What the renderer does |
| --- | --- |
| Theme tokens | `theme.tokens.surface` / `.onSurface` / `.font` override the `mode` defaults; every token falls back independently. The `font` token is recorded but never selects a host font: text always uses the built-in bitmap font. |
| Style effects | `gradient` (two-stop vertical linear gradient, integer lerp), `glass` (surface blended with the accent), `shadow` (0..8, offset darkening), `opacity` (0..1, exact fixed point 0..1024), `border` (1 px colour), `radius` (rounded corners), `margin`. Effects apply to every kind that owns a box (Container, Card, Button, …). |
| Conditional visibility | The optional node fields `when: "<boolState>"` and `unless: "<boolState>"` hide the node **and its subtree** from layout and from the raster while the state is false for `when` or true for `unless`. A field on a non-bool or undeclared state is a parse error. |
| Node kinds | All 24 interactive UI kinds render: the eight v0.1 kinds plus `Card, Divider, Icon, Badge, Progress, Switch, Slider, Checkbox, Radio, Dialog, Tabs, List, Grid, Scroll, Avatar, Spinner`. `Grid` uses `columns` for a fluid grid, `Scroll` clips to `height`, `List` is a vertical stack, `Progress`/`Slider` draw a track plus fill, `Switch`/`Checkbox`/`Radio` draw a box plus indicator. An unknown kind degrades to the documented fallback (Container with children, else Text using label/value/title/name) instead of failing. |
| Animation | `animate: spring\|ease\|inertia\|none` on a node drives an entry animation (box grows from its centre while fading in; `inertia` flings it under a constant integer braking force). The clock is the only time source: `Animator::tick(dt_ms)` advances it and `Renderer::render_at` draws the frame for that clock position. |

Unknown props are ignored and never fatal; an unknown kind degrades instead of
rejecting the file. Multi-action events (`{"actions": [...]}`) and
`valueFrom` are accepted as well, so a compiler output renders here.

## Inverted conditional visibility (LANGUAGE_ADDITIONS section 2)

The optional node field `"unless": "<boolState>"` keeps a node visible while that
bool state is false and hides the node and its subtree while the state is true.
It is the inverse of `when`, so one bool state selects between two sibling
subtrees:

```json
"children": [
  { "id": 1, "kind": "Text", "props": { "value": "light" }, "unless": "dark",
    "children": [], "events": {} },
  { "id": 2, "kind": "Text", "props": { "value": "dark" }, "when": "dark",
    "children": [], "events": {} }
]
```

Both fields may appear on one node, in which case the node is visible only while
its `when` state is true and its `unless` state is false. Each field must name a
declared bool state; an undeclared name, a non-bool state or a non-string value
is a readable parse diagnostic with exit code 1. Hidden nodes are omitted from
the layout, the raster, the display list and the SVG drawing; the SVG backend
records a `data-hidden` marker and the accessibility tree reports `hidden: true`
for the node and its descendants. The field is opt-in: an IR document without it
renders byte-identically, and the three golden digests are unchanged.

## Five-tier adaptive layout and fluid grid (TRANSPORT_AND_LANGUAGE_ADDITIONS §3)

The viewport width selects one of five tiers, on logical pixels:

| Tier | Width |
| --- | --- |
| Compact | `< 600` |
| Medium | `< 900` |
| Expanded | `< 1200` |
| Wide | `< 1600` |
| Ultra | `>= 1600` |

`Breakpoint::from_width` is the single entry point. The compact and medium tiers
keep their earlier behaviour exactly, so the three golden digests are unchanged.

### Fluid grid

`fluid_grid(width)` derives the column count, the gutter and the margin from a
container width with integer arithmetic only — no float, no randomness, no wall
clock, so the metrics are identical on every host:

```
columns = clamp(width / 120, 1, 12)
margin  = clamp(width /  40, 8, 32)
gutter  = clamp(width /  60, 8, 24)
```

A `Grid` node without an explicit `columns` prop uses the fluid grid from the
expanded tier upwards; the compact and medium tiers keep the fixed two-column
grid, and an explicit `columns` always wins at every tier. The remainder pixels
of a row go to the leftmost columns, so the cells always fill the container.

### Resize diffing

`Renderer::resize(width, height)` changes the viewport the way a window resize
or a split-screen split does, and keeps the buffered frame. The next render
diffs the shared region against it and reports the strip the new surface gained,
so the dirty list names the regions that actually changed. An unchanged
viewport still yields zero dirty rects, and the list stays bounded at 64 rects.
Emitted rects are ordered by `(y, x)`, so identical state changes produce identical
dirty lists.

## Accessibility (COMPONENTS_AND_A11Y §2)

All accessibility keys are **additive and opt-in**; a v0.1 IR without them renders
byte-identically to the core release (the three golden digests below are unchanged).

| Key | Type | Effect |
| --- | --- | --- |
| `fontScale` | number `0.5..3.0` | multiplies text size in integer logical pixels using exact fixed point (1024 = 1.0); the 5x7 bitmap glyph block grows in whole pixels. Out-of-range or non-number values are a readable parse diagnostic with exit 1. |
| `highContrast` | bool | forces pure black/white text and 2 px black/white borders on box-owning nodes; a node flag applies to its whole subtree. |
| `label` | string | accessible name; defaults to drawn text, then `label`/`value`/`title`/`name`/`placeholder`, then the empty string. |
| `role` | string | `button`, `text`, `image`, `checkbox`, `slider`, `heading`, `none`; an unknown value is a parse diagnostic. |
| `theme.tokens.contrast` | `"normal"\|"high"` | theme-wide high contrast, independent of the `mode` tokens. |

Default roles: `Button` -> `button`, `Text` -> `text` (or `heading` when its
effective size is >= 24 logical px), `Image` -> `image`, `Switch`/`Checkbox` ->
`checkbox`, `Slider` -> `slider`, everything else -> `none`. An explicit `role`
always wins, including `role: "none"`.

CLI:

```sh
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --a11y tree.json
```

`--a11y <out.json>` writes a deterministic pre-order JSON array; stdout stays
exactly `digest=...` and `dirty=...`. Each entry has the shape
`{"id":0,"role":"none","label":"","bounds":[0,0,800,600],"hidden":false}`:
integer bounds are the node's laid-out `[x,y,w,h]` in logical pixels. Hidden
conditional nodes and their descendants appear with `hidden:true`; their bounds
come from a second layout with every bool state forced true, or from zeros when
that layout cannot place them. The JSON is byte-identical across identical runs.

Status: this is a **data-only** accessibility tree. There is NO OS
screen-reader integration, NO ARIA/AT-SPI bridge, NO focus order or hit
testing, NO real font shaping, NO DPI scaling (`fontScale` scales logical
pixels of the built-in bitmap font) and NO GPU backend.

## SVG 1.1 vector backend (TOOLCHAIN_AND_PACKAGES §4)

`opurom-render <ir> <w> <h> <out.ppm> --svg <out.svg>` (or the `svg`/`svg_at`
library functions) writes a **byte-deterministic SVG 1.1 snapshot of the same
`layout()` pre-order boxes**:

- `<svg ... viewBox="0 0 W H">` root with a `version="1.1"` declaration and a
  background `<rect>` from the theme palette.
- One `<rect>` per box-owning node with `x/y/width/height`, `rx/ry` (rounded
  corners), `fill` (or a `stroke`/`stroke-width` border), and `opacity` for the
  fixed-point alpha.
- A `<defs>` `<linearGradient>` (two `<stop>`s) whenever a node has `gradient`;
  the rect then uses `fill="url(#gN)"`. Gradient ids are assigned in layout
  order, so the document is stable.
- `<text>` for text nodes, positioned and sized from the **built-in font
  metrics** (`font-size = GLYPH_H * text_scale`), with XML escaping and
  `{name}` interpolation already applied by layout.
- `data-id` and `data-role` on every element; the role is the shared a11y role
  (`role_of`). Hidden conditional nodes are omitted from the drawing and
  recorded as `<g ... data-hidden="true"/>` markers.

Same IR + viewport + state + theme + animation tick ⇒ byte-identical SVG bytes
across runs (integer formatting over a deterministic pre-order layout; no time,
randomness, host queries or external crate). Verified by tests for
determinism, gradient defs, text, `data-role`, and hidden nodes.

The SVG path is a **second software backend**. `<text>` uses a generic `monospace`
face and the built-in 5x7 font's metrics, so glyph widths/positions are an
approximation of the raster rather than a pixel-exact match. Box colours come
from the layout palette; `Image` stays a deterministic placeholder. It is a
vector *snapshot* of the layout, not a replacement for the RGBA conformance
oracle.

## Eight new distinct primitives (TOOLCHAIN_AND_PACKAGES §4)

Eight **distinct, drawn** primitives (not aliases and not the Container/Text
fallback) bring the total to **32 known node kinds**. Each has a documented prop
schema, a distinct deterministic raster, an a11y default role, and a test
asserting its pixels differ from the fallback and are deterministic. All are
leaves; all new props are optional and opt-in, so a v0.1 IR renders
byte-identically to the core release.

| Kind | Prop schema (all optional) | Deterministic drawing | Default a11y role |
| --- | --- | --- | --- |
| `Table` | `columns` (int, default 3), `rows` (int, default 3), `labels` (header strings), `values` (cell ints) | header band + rows/columns grid lines + numbered cells | `none` |
| `Chart` | `values` (int array or `"a,b,c"` string; default deterministic bars `[3,8,5,10,6,4]`) | baseline + vertical bars scaled to the max value, alternating accent/lerp shades | `image` |
| `Calendar` | `columns` (int, default 7), `rows` (int, default 5) | weekday header + month grid of day numbers + a deterministic "today" highlight | `none` |
| `RichText` | `value` (text) | greedy word-wrapped multi-line segments using the built-in font | `text` |
| `Player` | — | horizontal track + progress fill + play triangle + knob | `button` |
| `Map` | — | land fill + grid roads + diagonal route + ringed marker | `image` |
| `Menu` | `labels` (item strings; default `File/Edit/View`) | stacked item rows with separators and a highlighted first row | `none` |
| `Tooltip` | `value` or `label` (text) | rounded dark bubble + downward tail triangle + centered text | `text` |

New typed props: `values` (`Vec<i64>`), `rows` (`Option<i64>`); `columns` and
`labels` are reused from the interactive UI layer. Unknown props remain ignored;
an unknown kind still degrades to the documented fallback. The 108-name component catalog gains eight
names mapping onto these primitives (that catalog lives in the compiler repo,
outside this crate).

## Deterministic animation (GPU_WINDOW_TRANSPORT section 3)

All animation is additive and opt-in. The fixed-point unit is `ONE = 1024`;
there are no floats, no `rand` crate, no wall clock and no unseeded randomness.
The caller supplies every tick, gesture delta, particle seed and frame budget.
V0.1 IR output without these APIs is byte-identical to the core release.

- **Page transitions:** `Transition { kind, progress }` owns `slide`, `fade`
  or `scale`. `tick(dt_ms)` advances progress linearly by
  `dt_ms * 1024 / 320`, clamps to `0..=1024`, is monotonic and settles at 1024.
  `compose(from, to)` composes two existing RGBA frames: slide translates both
  pages by integer columns, fade blends channels in fixed point, and scale
  draws a centered nearest-neighbour copy of the incoming page over the
  outgoing page. At 1024 the result is exactly the incoming frame. The CLI uses
  the frame at tick 0 as the outgoing page and the settled frame as the
  incoming page; a given `--transition <kind> --tick <ms>` pair therefore has a
  reproducible frame digest.
- **Gesture-linked animation:** `GestureAnim::drag(delta_x, delta_y)` maps
  cumulative integer L1 displacement to integer progress `0..=1024`; one input
  unit is one progress unit and the value saturates. `compose(from, to)` scrubs
  the same integer slide at that progress. The same gesture sequence produces
  the same progress and digest sequence. This is an integer input API, not an
  OS gesture recognizer or real gesture hardware.
- **Fixed-seed particles:** `ParticleSystem::new(seed, count)` fills integer
  positions, velocities and lifetimes from a seeded xorshift64* generator. The
  integer field is 1024 units, lifetime is `1..=64` `step()` calls, and motion,
  wrapping and respawning are deterministic. `step()` (exactly
  `step_mode(StepMode::Wrap)`) stays the default motion source;
  `render_into(&mut Frame)` overlays 2x2 RGBA blocks on an existing
  frame. Identical seeds and step sequences give byte-identical frames.
- **Inertia curve:** `animate: inertia` flings the entry box at start velocity
  `FLING = 256` fixed-point units and brakes it by `BRAKE = 32` units every
  `STEP_MS = 16` ms: step `k` travels `FLING - k * BRAKE` and the distance is
  that arithmetic partial sum, clamped at 1024. The rule is pure integer
  arithmetic, so a fling settles in a finite, measurable time — the tests
  derive the counts from the formula and assert them against the real curve:
  settle after exactly 6 steps (96 ms), velocity at rest after 8 steps.
- **Bounce collision mode:** `step_mode(StepMode::Bounce)` is opt-in and keeps
  the wrap behaviour of `step()` untouched. A particle crossing the 1024-unit
  field boundary mirrors back with fixed-point restitution `e = 3/4`
  (`v' = -v * 768 / 1024`: a measured wall hit returns speed 6 as -4), and
  each particle checks its next 8 index neighbours within a 16-unit contact
  range (twice the maximum spawn speed, so a closing pair cannot tunnel in one
  step) for an equal-mass elastic response that exchanges the closing axis
  velocities. One boundary pass plus the bounded neighbour window keeps a
  large particle count at O(n), and identical seeds with identical step
  sequences compare equal field for field.
- **Automatic degradation:** `choose_quality(measured_frame_budget_ms,
  particle_count, effects_enabled) -> Quality` is a pure integer policy. The
  caller applies `Quality { particle_count, effects_enabled }` when
  constructing particles and enabling expensive effects.

| Measured budget (ms) | Particle count | Expensive effects |
| --- | --- | --- |
| `>=16` | full `particle_count` | as requested |
| `8..=15` | `3/4 * particle_count` | disabled |
| `4..=7` | `1/2 * particle_count` | disabled |
| `<4` | `1/4 * particle_count` | disabled |

Division truncates in `usize`; negative budgets use the lowest tier. **The
budget is supplied by the caller** (for example `opurom profile`) and is never
read from a clock inside the renderer. `opurom profile` remains the measured
source; this crate makes no measurement and no fps claim.

Transitions and particles are CPU RGBA software compositing and gesture deltas
are integers; quality tiers are a deterministic policy, not runtime device
profiling. The renderer does not sample `Instant`, system time or GPU counters.
`--a11y` and `--svg` snapshots are unchanged by `--transition`.

## Determinism guarantee

Same IR + same viewport + same state + same animation tick ⇒ byte-identical RGBA
pixels on every host (and, for `--svg`, byte-identical SVG). The animation APIs
add their own explicit inputs: transition ticks, gesture deltas, particle seeds/step
sequences and caller-supplied budgets. Mechanisms: all layout, raster and
animation arithmetic is integer (`i64`/`i128`, no float ordering, no `f32`
rounding); state lives in a `BTreeMap`; events are visited in sorted key
order; `theme.mode == "system"` resolves to light instead of querying the host;
no wall-clock time, unseeded randomness, threads, environment variables or
resource I/O affect the frame (`Image` uses a stable hash of `src` as a
placeholder colour); the animation clock only moves when the caller ticks it,
so two identical tick sequences produce identical digest sequences. The SVG
backend applies the same rules to its element/attribute ordering.

Golden digests (current text rasterization; additive layers preserve these values):

| Fixture | Viewport | Digest |
| --- | --- | --- |
| `counter.opuir.json` | 320x240 | `900abf4bd93d1e51` |
| `counter.opuir.json` | 800x600 | `c8403ba57da5ee61` |
| `counter.opuir.json`, `--state count=7` | 800x600 | `48a56056bd32a07e` |

`FrameView<'a> { width: u32, height: u32, pixels: &'a [u8] }` has the exact field
layout of `ADAPTER_CONTRACT.md`. It is a local struct: adapters do not gain a
compile-time dependency on this crate, and this crate does not import theirs.

## GPU display-list proof

The GPU backend lives in the independent `opurom-gpu` repository. This crate
exports the exact raster command stream with `--commands`; `opurom-gpu raster
<commands.json> <out.ppm>` uploads that stream to a real Metal compute kernel,
dispatches one thread per viewport pixel, reads back RGBA8 and writes the same
P6 PPM format. For the rect/gradient fixture, the independent software PPM and
Metal PPM are byte-identical: `cmp` exits 0. The golden `counter.opuir.json`
frame is also covered because CPU-decomposed glyph blocks are recorded as solid
rect commands. This is a real cross-repo GPU/software pixel proof, not a
self-comparison.

Layout, glyph decomposition and page transitions remain CPU-side; the GPU kernel
executes generic solid/rounded/gradient rectangles. `Frame::view()` and dirty
rects are unchanged; `--transition` composes whole CPU frames and cannot be
exported as this display list. Optional effect commands sit on top of this
stream; effect-free `--commands` output stays byte-identical.

## Deterministic software visual effects

All effects are **opt-in** through a node's `props.effects` array. Effects absent
leave the three golden digests and the display list unchanged.
Example:

```json
"props": {
  "width": 120,
  "height": 48,
  "effects": [
    {"kind":"blur","radius":4},
    {"kind":"shadow","dx":6,"dy":6,"alpha":512},
    {"kind":"glass","radius":3,"alpha":384,"color":"#FFFFFF"}
  ]
}
```

| Field | Type / default | Meaning |
| --- | --- | --- |
| `kind` | required string | One of `blur`, `shadow`, `glow`, `glass`, `liquid`. |
| `radius` | integer, default `3`, clamped to `0..=32` | Box-blur / silhouette / backdrop radius. Ignored by `liquid`. |
| `dx`, `dy` | integer, default `radius / 2`, clamped to `-64..=64` | Shadow silhouette offset. |
| `alpha` | integer fixed point, default `512` (`0.5`), clamped to `0..=1024` | Converted to byte alpha with `(alpha * 255 + 512) / 1024`. |
| `color` | optional `#RRGGBB` | Shadow defaults to black, glow to the node's drawn ink (text colour, then border, fill, then accent), glass/liquid tint to white. |

Unknown fields in an effect object are ignored. A non-array `effects` prop,
unknown `kind`, non-integer numeric field or invalid colour is a readable parse
error. Local effects are applied in array order at the node's draw step; each
`blur` is a final whole-frame pass in pre-order layout order, so children drawn
after a blur node are included. No float arithmetic, wall clock, randomness,
threads or external crate is used. Alpha compositing is integer half-up:
`(src * a + dst * (255 - a) + 127) / 255`.

| Kind | Deterministic software operation |
| --- | --- |
| `blur` | Separable horizontal then vertical integer box blur over the complete RGBA8 frame. Window ends are clamped to the frame, so edge windows have fewer samples and out-of-bounds samples are omitted, not repeated. For each channel, `(sum + count / 2) / count` is the exact half-up rounding rule (ties round up); alpha stays 255. |
| `shadow` | A radius-softened binary silhouette of the node is offset by `dx`/`dy` and alpha-composited behind the node with a dark colour. |
| `glow` | The same silhouette is box-blurred; coverage outside the node scales an alpha-composited halo in the node's ink colour. |
| `glass` | The backdrop snapshot taken before the node is box-blurred and alpha-composited with a translucent tint inside the node's rounded bounds. This replaces the node's opaque fill; child nodes still draw afterwards. The older `glass: true` prop remains a colour blend, not a blur. |
| `liquid` | A diagonal triangular specular band, `band = 1024 - abs(3 * t - 1024)` with `t = diagonal * 1024 / (width + height - 2)`, scales a fixed-point white/highlight alpha over the node bounds. |

The CPU RGBA implementation is a framebuffer effect path. `liquid` is a stylized
specular gradient and `glass` uses a framebuffer snapshot. Effects are not
represented in the SVG snapshot or the a11y tree. A Metal compute path for the
five effects exists in the GPU repository (see below).

## Deterministic particle fields

A node opts into a particle field with the `particles` prop:

```json
"props": {
  "width": 96,
  "height": 64,
  "color": "#223344",
  "particles": {"count": 40, "seed": 7, "radius": 2, "alpha": 1024, "color": "#FF0044"}
}
```

| Field | Type / default | Meaning |
| --- | --- | --- |
| `count` | required integer `0..=512` | Number of dots. `0` paints nothing. |
| `seed` | integer `0..=4294967295`, default `1` | Selects every dot position; different seeds produce different fields. |
| `radius` | integer `0..=32`, default `2` | Dot radius in pixels; coverage is the integer circle test `dx*dx + dy*dy <= radius*radius`. |
| `alpha` | fixed point `0..=1024`, default `1024` | Blend alpha, reduced to a byte as `alpha * 255 / 1024`; `0` paints nothing. |
| `color` | optional `#RRGGBB` | Dot colour; defaults to the node accent. |

Particles are a deterministic spatial primitive. There is no time input, no
motion, no physics and no wall clock: dot `i` is placed at

```text
cx = rect.x + mix(seed ^ (i * A + B)) mod rect.w
cy = rect.y + mix(seed ^ (i * A + C)) mod rect.h
```

where `A`, `B` and `C` are fixed 32-bit constants and `mix` is an integer
xorshift/multiply avalanche over `u32` (`x ^= x >> 16`, `x *= 0x7feb352d`,
`x ^= x >> 15`, `x *= 0x846ca68b`, `x ^= x >> 16`). Only the seed and the dot
index select positions; the viewport, frame buffer and other commands cannot
change them. Each dot is painted in index order with the same integer `/255`
blend as a solid rect, clipped to the node rect, the effective clip and the
viewport. The field is drawn after the node's box, control and text, and before
any local framebuffer effects, so a local effect silhouette includes the dots.
The node opacity entry animation scales the box fill and leaves the particle
alpha at its prop value.

Every out-of-range or non-integer field, a non-object prop and an invalid
colour are readable parse errors. Nodes without the prop emit no particle
command, and the three golden digests are unchanged. The display list gains one
`{"kind":"particles","rect":[...],"clip":[...],"count":...,"seed":...,"radius":...,"color":[r,g,b,a]}`
command per node with a visible field; the cross-repo GPU backend evaluates the
same dot centres, circle test and blend order per pixel. The SVG snapshot and
the accessibility tree draw the node box but not its particle field.

## GPU effect kernels (cross-repo proof)

`--commands` now accepts effect IR. The command stream gains seven fixed-shape
effect commands with integer parameters only: `effect_backdrop` (snapshot the
frame before a node's draws), `effect_mask` (snapshot the rounded silhouette
from the frame/backdrop diff), `effect_shadow`, `effect_glow`, `effect_glass`,
`effect_liquid` (one per local effect in IR order), and `effect_blur` (whole-frame
passes appended after all nodes, in pre-order layout order). Effect-free IR emits
none of them: `tests/fixtures/counter-commands.json` still matches
byte-for-byte, and the three golden digests are unchanged.

The independent `opurom-gpu` repository executes that stream in real MSL
compute kernels: one thread per viewport pixel, multiple dispatched passes for
frame snapshots, silhouette masks, local effects and separable whole-frame
blurs, with real buffer readback. For the committed effect fixture
`tests/fixtures/effects.opuir.json` at `96 64`, the real cross-repo proof is:

```sh
./target/debug/opurom-render tests/fixtures/effects.opuir.json 96 64 cpu.ppm \
  --commands fx.commands.json     # digest=ab00722ca3ae679a, dirty=1
../opurom-gpu/target/debug/opurom-gpu raster fx.commands.json gpu.ppm
cmp cpu.ppm gpu.ppm                # exit 0, byte-identical
```

Measured result: `cmp` exits 0, both PPMs are 18445 bytes, and the FNV-1a 64
digest of the PPM bytes is `292043461093fcd5` (all five effect kinds active).
`tests/gpu_effects.rs` also proves the same equality at `64 48` (9229 bytes, PPM
digest `51ba485cdab48339`). This is a real comparison against the independent
software renderer, not a self-comparison.

The GPU path is compute-kernel only: glyphs remain CPU-decomposed rects. The CPU
renderer remains the reference oracle.

## 68 additional distinct primitives (COMPONENT_LIBRARY_HARDENING section 1)

The library extends from 116 named components / 32 primitives to **216 named
components mapped onto 100 DISTINCT primitive kinds**: the 32 core kinds plus 68
new ones. Each new kind is a real, deterministic software drawing in
[`src/primitives.rs`](src/primitives.rs) (with its generated
[`src/primitives/specs.rs`](src/primitives/specs.rs) table), **not** an alias,
**not** the Container/Text fallback and **not** a parameter-only variation of an
existing kind. There are no new crates: every pixel is written with integer
`i64`/`i128` arithmetic, no wall clock, no `rand`, no floats and no host queries.

The implementation is a small shared raster engine, not 68 copies of one
function: 25 distinct geometric motifs (bars, polyline, area, pie, donut,
scatter, stacked segments, rings, spokes, grid, heatmap, steps, triangle wave,
ruler, crosshair, chevrons, gauge, treemap blocks, corner brackets, spectrum,
ladder, dot matrix, checkerboard, pyramid) are parameterised by a per-kind code
and by the optional `values` integer series. A seven-cell identity band encodes
the kind code, so every kind's frame is pairwise-distinct by construction.

Parsed as `Kind::Extended(code)`, each new kind keeps the normal rounded box and
border, is a leaf, carries its documented prop schema in `primitives::SPECS`, and
has an explicit default a11y role (one of the seven standard roles). The SVG 1.1
backend emits the box and the correct `data-role`, and the a11y tree reports the
role and bounds. `tests/primitives.rs` proves all 68 differ from the fallback,
from every core kind and from each other, are deterministic across repeated renders and across
separate CLI processes, and that the three golden digests are unchanged.

| Family | Kinds |
| --- | --- |
| data / charts | Gauge, Sparkline, AreaChart, PieChart, DonutChart, ScatterPlot, StackedBar, Histogram, Heatmap, Treemap, Waterfall, BulletChart, FunnelChart, Waveform, Equalizer, Candlestick, BubbleChart, PolarChart |
| navigation | Breadcrumbs, Pagination, Stepper, Sidebar, TabStrip, NavRail, AnchorBar, ProgressSteps, MenuBar, TocTree |
| form / input | Rating, Dial, Knob, ColorWell, DateField, TimeField, SelectBox, ComboBox, SearchBar, PinInput, SegmentedInput, UploadZone |
| media | WaveformPlayer, Spectrogram, Filmstrip, ThumbnailGrid, QrGlyph, BarcodeGlyph, ColorPalette, ZoomLens |
| feedback / indicators | LevelMeter, SignalBars, BatteryMeter, Thermometer, Speedometer, Compass, ProgressRing, ActivityRing, KpiTile, StatCard |
| layout / decoration | CornerFrame, CrosshairMark, LadderDivider, ChevronDivider, GridOverlay, CheckerBackdrop, DotPattern, RingBadge, StairDivider, BracketPanel |

Full per-kind documentation (prop schema, motif, a11y role):

| # | Kind | Family | Geometric motif | Documented prop schema | Default a11y role |
| --- | --- | --- | --- | --- | --- |
| 0 | `Gauge` | data | Gauge | Gauge accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `slider` |
| 1 | `Sparkline` | data | LineChart | Sparkline accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 2 | `AreaChart` | data | AreaChart | AreaChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 3 | `PieChart` | data | Pie | PieChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 4 | `DonutChart` | data | Donut | DonutChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 5 | `ScatterPlot` | data | Scatter | ScatterPlot accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 6 | `StackedBar` | data | Stacked | StackedBar accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 7 | `Histogram` | data | Bars | Histogram accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 8 | `Heatmap` | data | GridCells | Heatmap accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 9 | `Treemap` | data | Blocks | Treemap accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 10 | `Waterfall` | data | Steps | Waterfall accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 11 | `BulletChart` | data | Ruler | BulletChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 12 | `FunnelChart` | data | Chevrons | FunnelChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 13 | `Waveform` | data | Wave | Waveform accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 14 | `Equalizer` | data | Spectrum | Equalizer accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 15 | `Candlestick` | data | Ladder | Candlestick accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 16 | `BubbleChart` | data | Rings | BubbleChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 17 | `PolarChart` | data | Spokes | PolarChart accepts value:int 0..100 (magnitude); values:int[] (series); segments:int; labels:string[]; min:int; max:int; color:#RRGGBB | `image` |
| 18 | `Breadcrumbs` | navigation | Chevrons | Breadcrumbs accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 19 | `Pagination` | navigation | DotMatrix | Pagination accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 20 | `Stepper` | navigation | Steps | Stepper accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 21 | `Sidebar` | navigation | Brackets | Sidebar accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `none` |
| 22 | `TabStrip` | navigation | HBars | TabStrip accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 23 | `NavRail` | navigation | Ladder | NavRail accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 24 | `AnchorBar` | navigation | Ruler | AnchorBar accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 25 | `ProgressSteps` | navigation | Chevrons | ProgressSteps accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `none` |
| 26 | `MenuBar` | navigation | HBars | MenuBar accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `button` |
| 27 | `TocTree` | navigation | Blocks | TocTree accepts value:int (active index); labels:string[] (items); segments:int; size:int; color:#RRGGBB | `none` |
| 28 | `Rating` | form | DotMatrix | Rating accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `slider` |
| 29 | `Dial` | form | Gauge | Dial accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `slider` |
| 30 | `Knob` | form | Donut | Knob accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `slider` |
| 31 | `ColorWell` | form | Checker | ColorWell accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `button` |
| 32 | `DateField` | form | GridCells | DateField accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `text` |
| 33 | `TimeField` | form | Rings | TimeField accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `text` |
| 34 | `SelectBox` | form | Brackets | SelectBox accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `button` |
| 35 | `ComboBox` | form | Chevrons | ComboBox accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `button` |
| 36 | `SearchBar` | form | Crosshair | SearchBar accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `text` |
| 37 | `PinInput` | form | Checker | PinInput accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `text` |
| 38 | `SegmentedInput` | form | Stacked | SegmentedInput accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `checkbox` |
| 39 | `UploadZone` | form | Brackets | UploadZone accepts value:int (current value); min:int; max:int; labels:string[]; segments:int; size:int; color:#RRGGBB | `button` |
| 40 | `WaveformPlayer` | media | Wave | WaveformPlayer accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `button` |
| 41 | `Spectrogram` | media | Heat | Spectrogram accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 42 | `Filmstrip` | media | HBars | Filmstrip accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 43 | `ThumbnailGrid` | media | GridCells | ThumbnailGrid accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 44 | `QrGlyph` | media | DotMatrix | QrGlyph accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 45 | `BarcodeGlyph` | media | Ladder | BarcodeGlyph accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 46 | `ColorPalette` | media | Stacked | ColorPalette accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 47 | `ZoomLens` | media | Rings | ZoomLens accepts value:int 0..100 (playhead); values:int[] (samples); src:string (placeholder only); width:int; height:int | `image` |
| 48 | `LevelMeter` | feedback | Spectrum | LevelMeter accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `slider` |
| 49 | `SignalBars` | feedback | Bars | SignalBars accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `image` |
| 50 | `BatteryMeter` | feedback | Stacked | BatteryMeter accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `image` |
| 51 | `Thermometer` | feedback | Pyramid | Thermometer accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `slider` |
| 52 | `Speedometer` | feedback | Gauge | Speedometer accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `slider` |
| 53 | `Compass` | feedback | Spokes | Compass accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `image` |
| 54 | `ProgressRing` | feedback | Donut | ProgressRing accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `image` |
| 55 | `ActivityRing` | feedback | Rings | ActivityRing accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `image` |
| 56 | `KpiTile` | feedback | Blocks | KpiTile accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `text` |
| 57 | `StatCard` | feedback | Ruler | StatCard accepts value:int 0..100 (level); min:int; max:int; labels:string[]; segments:int; color:#RRGGBB | `text` |
| 58 | `CornerFrame` | layout | Brackets | CornerFrame accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 59 | `CrosshairMark` | layout | Crosshair | CrosshairMark accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 60 | `LadderDivider` | layout | Ladder | LadderDivider accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 61 | `ChevronDivider` | layout | Chevrons | ChevronDivider accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 62 | `GridOverlay` | layout | GridCells | GridOverlay accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 63 | `CheckerBackdrop` | layout | Checker | CheckerBackdrop accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 64 | `DotPattern` | layout | Scatter | DotPattern accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 65 | `RingBadge` | layout | Rings | RingBadge accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 66 | `StairDivider` | layout | Steps | StairDivider accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |
| 67 | `BracketPanel` | layout | Brackets | BracketPanel accepts value:int (phase); columns:int; rows:int; segments:int; size:int; color:#RRGGBB | `none` |

Common prop schema keys (all optional): `value` (int magnitude, positional),
`values` (int series, JSON array or comma-separated string), `segments`,
`columns`, `rows`, `min`, `max`, `size`, `width`, `height`, `labels`, `src`,
`type`, `color`, plus the generic and accessibility props. `src` is a placeholder
only: no image is decoded.

Each kind has exactly one deterministic software drawing. The SVG and a11y
backends report the box and role for a new kind; they do not vectorise its motif
geometry.

## Verified status

Verified by tests in this crate: IR parsing/validation, adaptive breakpoints +
Row collapse + fluid sizing, the five viewport tiers with their exact
boundaries, the integer fluid grid arithmetic, resize diffing across a tier
change, all 100 node kinds, theme tokens with per-token
fallback, the six canonical style effects, conditional `when` visibility with
bounded dirty rects, inverted `unless` visibility in the layout, raster,
accessibility tree, SVG snapshot and CLI, light/dark theme + accent, built-in
5x7 font, deterministic RGBA8 software raster, incremental dirty-rect diffing
with coverage and bound tests, interpolation from `StateView`, `FrameView`
shape, the PPM CLI, integer `fontScale`, high-contrast rendering, the
`theme.tokens.contrast` token, a deterministic pre-order accessibility tree with
roles, labels, bounds and hidden
flags, a deterministic SVG 1.1 vector snapshot (rects, gradients, text,
`data-role`, hidden markers), eight data primitives and 68 extended primitives as
distinct deterministic drawings, slide/fade/scale frame composition with monotonic
ticks and settling, gesture digest sequences, fixed-seed particles, the inertia
curve's measured settle and rest step counts against the derived braking
formula, wall-bounce rebounds against the derived restitution formula,
equal-mass elastic velocity exchange with momentum conservation, byte-identical
bounce-mode state and frames for identical seeds and step counts, wrap mode
preserved as the default, quality budget tiers, CLI transition validation, CLI
`--tick` driving the inertia curve, exact display-list serialization/clipping/draw
order, the five software effects with determinism, pairwise distinct pixels,
clamping, overflow, effect-command export for all five kinds with integer
parameters, draw order, bounds, effect-free fixture invariance, and unchanged
golden digests; opt-in particle fields with byte-identical repeated renders,
seed sensitivity, `count=0` and `alpha=0` no-ops, bounded parse diagnostics,
rect clipping, effect-snapshot draw order and unchanged golden digests.
`cargo test --offline` (185 tests) is green with fixed golden digests and zero
warnings. The cross-repo Metal byte-equality proof lives in `opurom-gpu`.



## Roadmap

The renderer evolves through additive layers that keep the IR contract and the golden digests
stable. Planned areas of work:

- **Shaped text**: a font atlas with hinting, kerning and shaping, integrated with a GPU glyph
  path.
- **Image resources**: resource-backed image decoding in place of the deterministic colour
  placeholder.
- **Dirty-region refinement**: a tile quadtree for large framebuffers.
- **Gradient and compositor layers**: angled multi-stop gradients, compositor-backed glass and
  per-property keyframe animation.
- **Transition presentation**: compositor layers with a real presenter.
- **Hardware gesture events**: full gesture event streams in place of integer deltas.
- **Spatial contact queries**: spatial-hash contact queries for the step-based
  particle field.
- **Quality feedback**: profile-driven quality budgets from the host.
- **Platform accessibility bridge**: a native accessibility bridge over the layout-derived tree.
- **SVG text shaping**: real shaping and font metrics in the SVG backend.
