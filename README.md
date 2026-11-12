# opurom-render

Independent Rust crate: IR v0.1 → adaptive layout → deterministic RGBA8 software
raster → incremental dirty rects, plus a deterministic SVG 1.1 **vector software
backend**, deterministic R5 animation primitives, an exact R6 display-list
export and five R8 deterministic framebuffer effects, plus R9 optional effect
commands that reproduce them on the cross-repo Metal compute backend. This folder is its own
git repository; it has no source dependency on
any other OPUROM layer, only on the frozen IR/ABI contracts
(`IR_SCHEMA.md`, `ADAPTER_CONTRACT.md`) and local `FrameView` shape
compatibility.

## Verify

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd opurom-render
cargo fmt --all
cargo fmt --check
cargo test --offline
```

`cargo test --offline` runs 111 tests (unit + integration). All pass, zero
warnings. Only `serde_json` is used; it is pinned to the locally cached
`=1.0.154`, so the build needs no network.

CLI (integration surface for `opurom-cli` and the e2e suite):

```sh
cargo build --offline
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm
# digest=8cc3021fa80749b5
# dirty=1
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --state count=7
# digest=e7aad51c98d9cf21
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --a11y tree.json
# digest=8cc3021fa80749b5, dirty=1 (stdout unchanged)
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --svg out.svg
# writes a deterministic SVG 1.1 snapshot (stdout unchanged)
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 320 240 out.ppm --commands commands.json
# writes the exact R6 display list (stdout unchanged)
./target/debug/opurom-render tests/fixtures/r9_effect.opuir.json 96 64 fx.ppm --commands fx.commands.json
# writes the R6 display list plus R9 effect commands (stdout unchanged)
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --transition slide --tick 16
# deterministic transition digest (stdout still exactly digest=... and dirty=...)
```

Usage: `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick 0..4000] [--transition slide|fade|scale] [--a11y out.json] [--svg out.svg] [--commands out.json]`.
`--state` repeats; values are parsed against the declared state type. `--tick`
sets the animation clock position in milliseconds, accepts `0..=4000`, and
rejects non-numeric or out-of-range values with a readable diagnostic; the
default is the settled end state, so a v0.1 IR renders exactly as in round 1.
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
display list as deterministic JSON: solid rects, rounded rects and two-stop
vertical gradients with integer bounds/colours in draw order, plus the viewport
and opaque background. Bitmap glyph runs are decomposed on the CPU into solid
rect commands; there is no GPU glyph atlas or GPU text shaping. CPU-only page
transitions cannot be represented as this display list, so `--commands` and
`--transition` together fail with a readable diagnostic. R9 adds optional
effect commands for IR carrying R8 visual effects: `effect_backdrop` and
`effect_mask` snapshot the frame and the node silhouette, `effect_shadow`,
`effect_glow`, `effect_glass` and `effect_liquid` carry the clamped integer
parameters (radius, dx/dy, fixed-point alpha, colour, bounds, shape radius),
and `effect_blur` is a whole-frame pass appended after all nodes. Effect-free
IR emits none of them, so the frozen R6 command fixture stays byte-identical.
The flag adds no stdout lines and does not change the PPM or golden digests.

## Architecture

```
serde_json::Value ── parse_ir (validate) ──▶ Ir { state, tree: Node, theme }
                                               │
                           StateView (flat BTreeMap, {name} interpolation)
                                               │
                      layout(Ir, StateView, viewport) ──▶ Vec<LaidOut>  (pre-order)
                                               │
                               ├──▶ raster::Canvas (rounded fills/borders + 5x7 glyphs + local R8 effects) ──▶ final R8 frame blurs ──▶ Vec<u8> RGBA8
                               │                       │
                               │                       ├──▶ Vec<Command> ──▶ DisplayList JSON (R6)
                                │                       │
                                │       Renderer::render diff vs previous buffer ──▶ Frame { dirty }
                               │                       │
                               │               Frame::view() ──▶ FrameView<'_> for adapters
                               │
                               └──▶ svg::svg_at (same Vec<LaidOut>) ──▶ deterministic SVG 1.1 bytes
```

- `src/ir.rs` — strictly validates `ir == "0.1"`, state decls/types/inits, pre-order
  node ids, the 32 node kinds, known props, event actions (targets/types) and the
  theme. Unknown props are ignored. Errors are `IrError(String)` values.
- `src/layout.rs` — pure integer layout. Breakpoints on **logical px**: compact
  `<600`, medium `<900`, expanded `>=900`. `Row` collapses to a vertical stack in
  compact. Children share the main axis in proportion to their natural size (fluid
  sizing, i128 division with a deterministic remainder), honouring explicit
  `width`/`height`. `gap`, `padding`, `margin`, `align`, `radius` and `color` are
  resolved here.
- `src/font.rs` — built-in 5x7 bitmap font, printable ASCII `0x20..=0x7E` (A–Z,
  a–z, 0–9, punctuation). Unsupported codepoints consume a blank advance.
- `src/raster.rs` — filled rounded rectangles (integer quarter-circle test), 1 px
  borders, gradients, shadows, glass fills, scaled glyph blocks, opaque RGBA with
  integer alpha blending, the eight R4 primitive drawings and exact R6 command
  recording.
- `src/effects.rs` — R8 integer RGBA framebuffer effects: separable box blur,
  silhouette shadow/glow, frosted backdrop glass and a specular liquid gradient.
- `src/display.rs` — fixed-shape deterministic R6 display-list JSON with integer
  bounds/colours, draw order, viewport and opaque background.
- `src/anim.rs` — the fixed-timestep animation clock, spring/ease curves, R5
  page transitions, gesture progress, fixed-seed particles and quality tiers.
- `src/lib.rs` — `Rgba`, `StateView`, `Renderer`, `Frame`, `FrameView`, inline FNV-1a
  digest, and row/column dirty-rect diffing.
- `src/a11y.rs` — R3 pre-order accessibility tree, its deterministic JSON serializer,
  and the shared `role_of` helper used by the SVG `data-role` backend.
- `src/svg.rs` — R4 deterministic SVG 1.1 vector snapshot of the same layout.
- `src/main.rs` — the CLI described above.

## R2 features (IR_R2_ADDENDUM)

All R2 fields are **additive and opt-in**. A v0.1 IR file renders byte-identically
to round 1 — the three golden digests below are unchanged.

| Area | What the renderer does |
| --- | --- |
| Theme tokens | `theme.tokens.surface` / `.onSurface` / `.font` override the `mode` defaults; every token falls back independently. The `font` token is recorded but never selects a host font: text always uses the built-in bitmap font. |
| Style effects | `gradient` (two-stop vertical linear gradient, integer lerp), `glass` (surface blended with the accent), `shadow` (0..8, offset darkening), `opacity` (0..1, exact fixed point 0..1024), `border` (1 px colour), `radius` (rounded corners), `margin`. Effects apply to every kind that owns a box (Container, Card, Button, …). |
| Conditional visibility | The optional node field `when: "<boolState>"` hides the node **and its subtree** from layout and from the raster while the state is false. A `when` on a non-bool or undeclared state is a parse error. |
| Node kinds | All 24 R2 kinds render: the eight v0.1 kinds plus `Card, Divider, Icon, Badge, Progress, Switch, Slider, Checkbox, Radio, Dialog, Tabs, List, Grid, Scroll, Avatar, Spinner`. `Grid` uses `columns` for a fluid grid, `Scroll` clips to `height`, `List` is a vertical stack, `Progress`/`Slider` draw a track plus fill, `Switch`/`Checkbox`/`Radio` draw a box plus indicator. An unknown kind degrades to the documented fallback (Container with children, else Text using label/value/title/name) instead of failing. |
| Animation | `animate: spring\|ease\|none` on a node drives an entry animation (box grows from its centre while fading in). The clock is the only time source: `Animator::tick(dt_ms)` advances it and `Renderer::render_at` draws the frame for that clock position. |

Unknown props are ignored and never fatal; an unknown kind degrades instead of
rejecting the file. Multi-action events (`{"actions": [...]}`) and
`valueFrom` are accepted as well, so an R2 compiler output renders here.

## R3 accessibility (R3_ADDENDUM §2)

All R3 keys are **additive and opt-in**; a v0.1 IR without them renders
byte-identically to round 1 (the three golden digests below are unchanged).

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
`when` nodes and their descendants appear with `hidden:true`; their bounds come
from a second layout with every bool state forced true, or from zeros when that
layout cannot place them. The JSON is byte-identical across identical runs.

Status: this is a **data-only** accessibility tree. There is NO OS
screen-reader integration, NO ARIA/AT-SPI bridge, NO focus order or hit
testing, NO real font shaping, NO DPI scaling (`fontScale` scales logical
pixels of the built-in bitmap font) and NO GPU backend.

## R4 SVG 1.1 vector backend (R4_ADDENDUM §4)

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
  (`role_of`). Hidden `when` nodes are omitted from the drawing and recorded as
  `<g ... data-hidden="true"/>` markers.

Same IR + viewport + state + theme + animation tick ⇒ byte-identical SVG bytes
across runs (integer formatting over a deterministic pre-order layout; no time,
randomness, host queries or external crate). Verified by tests for
determinism, gradient defs, text, `data-role`, and hidden nodes.

**Honest scope:** the SVG path is a **second software backend**, NOT a
Metal/Vulkan/ANGLE GPU backend and not a hardware presenter. It does **no real
font shaping** — `<text>` uses a generic `monospace` face and the built-in
5x7 font's metrics, so glyph widths/positions are an approximation, not a
pixel-exact match to the raster — and it does **no image decoding** (box colours
come from the layout palette; `Image` stays a deterministic placeholder). It is
a vector *snapshot* of the layout, not a replacement for the RGBA conformance
oracle.

## R4 eight new distinct primitives (R4_ADDENDUM §4)

R4 adds eight **distinct, drawn** primitives (not aliases and not the
Container/Text fallback), bringing the total to **32 known node kinds**. Each
has a documented prop schema, a distinct deterministic raster, an a11y default
role, and a test asserting its pixels differ from the fallback and are
deterministic. All are leaves; all new props are optional and opt-in, so a
v0.1 IR renders byte-identically to round 1.

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
`labels` are reused from R2. Unknown props remain ignored; an unknown kind still
degrades to the documented fallback. The 108-name component catalog gains eight
names mapping onto these primitives (that catalog lives in the compiler repo,
outside this crate).

## R5 deterministic animation (R5_ADDENDUM section 3)

All R5 animation is additive and opt-in. The fixed-point unit is `ONE = 1024`;
there are no floats, no `rand` crate, no wall clock and no unseeded randomness.
The caller supplies every tick, gesture delta, particle seed and frame budget.
V0.1 IR output without these APIs is byte-identical to round 1.

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
  wrapping and respawning are deterministic. `step()` is the only motion
  source; `render_into(&mut Frame)` overlays 2x2 RGBA blocks on an existing
  frame. Identical seeds and step sequences give byte-identical frames.
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

**Honest scope:** transitions and particles are CPU RGBA software compositing,
not a GPU compositor; gesture deltas are integers, not real touch hardware;
quality tiers are a deterministic policy, not runtime device profiling. The
renderer does not sample `Instant`, system time or GPU counters. No frame-rate
or fps claim is made. `--a11y` and `--svg` snapshots are unchanged by
`--transition`.

## Determinism guarantee

Same IR + same viewport + same state + same animation tick ⇒ byte-identical RGBA
pixels on every host (and, for `--svg`, byte-identical SVG). R5 APIs add their
own explicit inputs: transition ticks, gesture deltas, particle seeds/step
sequences and caller-supplied budgets. Mechanisms: all layout, raster and
animation arithmetic is integer (`i64`/`i128`, no float ordering, no `f32`
rounding); state lives in a `BTreeMap`; events are visited in sorted key
order; `theme.mode == "system"` resolves to light instead of querying the host;
no wall-clock time, unseeded randomness, threads, environment variables or
resource I/O affect the frame (`Image` uses a stable hash of `src` as a
placeholder colour); the animation clock only moves when the caller ticks it,
so two identical tick sequences produce identical digest sequences. The SVG
backend applies the same rules to its element/attribute ordering.

Golden digests (unchanged by R2, R3, R4 and R5):

| Fixture | Viewport | Digest |
| --- | --- | --- |
| `counter.opuir.json` | 320x240 | `c87bf023a6c01f9d` |
| `counter.opuir.json` | 800x600 | `8cc3021fa80749b5` |
| `counter.opuir.json`, `--state count=7` | 800x600 | `e7aad51c98d9cf21` |

`FrameView<'a> { width: u32, height: u32, pixels: &'a [u8] }` has the exact field
layout of `ADAPTER_CONTRACT.md`. It is a local struct: adapters do not gain a
compile-time dependency on this crate, and this crate does not import theirs.

## R6 GPU display-list proof

The R6 GPU backend lives in the independent `opurom-gpu` repository. This crate
exports the exact raster command stream with `--commands`; `opurom-gpu raster
<commands.json> <out.ppm>` uploads that stream to a real Metal compute kernel,
dispatches one thread per viewport pixel, reads back RGBA8 and writes the same
P6 PPM format. For the rect/gradient fixture, the independent software PPM and
Metal PPM are byte-identical: `cmp` exits 0. The golden `counter.opuir.json`
frame is also covered because CPU-decomposed glyph blocks are recorded as solid
rect commands. This is a real cross-repo GPU/software pixel proof, not a
self-comparison.

Honest scope: layout, glyph decomposition and page transitions remain CPU-side;
the GPU kernel executes generic solid/rounded/gradient rectangles only. There is
no GPU glyph atlas, full-scene GPU text, GPU presentation to a window,
Vulkan/ANGLE/D3D backend, device matrix or fps claim. `Frame::view()` and dirty
rects are unchanged; `--transition` composes whole CPU frames and cannot be
exported as this display list. R9 adds optional effect commands on top of this
stream; effect-free `--commands` output stays byte-identical to R6.

## R8 deterministic software visual effects

All R8 effects are **opt-in** through a node's `props.effects` array. Effects
absent leave the three V0.1 golden digests and the R6 display list unchanged.
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
| `glass` | The backdrop snapshot taken before the node is box-blurred and alpha-composited with a translucent tint inside the node's rounded bounds. This replaces the node's opaque fill; child nodes still draw afterwards. The old R2 `glass: true` prop remains a colour blend, not a blur. |
| `liquid` | A diagonal triangular specular band, `band = 1024 - abs(3 * t - 1024)` with `t = diagonal * 1024 / (width + height - 2)`, scales a fixed-point white/highlight alpha over the node bounds. |

**Honest scope:** the CPU RGBA implementation is a framebuffer effect path, not
a compositor layer tree. `liquid` is a stylized specular gradient, not real lens
refraction or physically based transmission; `glass` uses a framebuffer
snapshot, not real depth-aware refraction. There is no real-time 120fps claim
and no measured frame rate. Effects are not represented in the R4 SVG snapshot
or the R3 a11y tree. R9 adds a Metal compute path for the five effects (see
below); presentation, GPU text and real lens optics remain out of scope.

## R9 GPU effect kernels (cross-repo proof)

`--commands` now accepts effect IR. The command stream gains seven fixed-shape
effect commands with integer parameters only: `effect_backdrop` (snapshot the
frame before a node's draws), `effect_mask` (snapshot the rounded silhouette
from the frame/backdrop diff), `effect_shadow`, `effect_glow`, `effect_glass`,
`effect_liquid` (one per local effect in IR order), and `effect_blur` (whole-frame
passes appended after all nodes, in pre-order layout order). Effect-free IR emits
none of them: `tests/fixtures/r6_counter_commands.json` still matches
byte-for-byte, and the three V0.1 golden digests are unchanged.

The independent `opurom-gpu` repository executes that stream in real MSL
compute kernels: one thread per viewport pixel, multiple dispatched passes for
frame snapshots, silhouette masks, local effects and separable whole-frame
blurs, with real buffer readback. For the committed effect fixture
`tests/fixtures/r9_effect.opuir.json` at `96 64`, the real cross-repo proof is:

```sh
./target/debug/opurom-render tests/fixtures/r9_effect.opuir.json 96 64 cpu.ppm \
  --commands fx.commands.json     # digest=eb9ce1cec41bac13, dirty=1
../opurom-gpu/target/debug/opurom-gpu raster fx.commands.json gpu.ppm
cmp cpu.ppm gpu.ppm                # exit 0, byte-identical
```

Measured result: `cmp` exits 0, both PPMs are 18445 bytes, and the FNV-1a 64
digest of the PPM bytes is `b493cd0714a4fb64` (all five effect kinds active).
`tests/r9.rs` also proves the same equality at `64 48`. This is a real
comparison against the independent software renderer, not a self-comparison.

**Honest scope:** compute kernels only. There is no GPU presentation to a
window, no GPU text (glyphs remain CPU-decomposed rects), no Vulkan/ANGLE/D3D
backend and no fps claim. The CPU renderer remains the reference oracle.

## Honest status (round 9)

Verified now, by tests in this crate: IR parsing/validation, adaptive breakpoints +
Row collapse + fluid sizing, all 32 node kinds, theme tokens with per-token
fallback, the six canonical style effects, conditional `when` visibility with
bounded dirty rects, light/dark theme + accent, built-in 5x7 font, deterministic
RGBA8 software raster, incremental dirty-rect diffing with coverage and bound
tests, interpolation from `StateView`, `FrameView` shape, the PPM CLI, integer
`fontScale`, high-contrast rendering, the `theme.tokens.contrast` token, a
deterministic pre-order accessibility tree with roles, labels, bounds and hidden
flags, a deterministic SVG 1.1 vector snapshot (rects, gradients, text,
`data-role`, hidden markers), eight distinct deterministic primitives, R5
slide/fade/scale frame composition with monotonic ticks and settling, gesture
digest sequences, fixed-seed particles, quality budget tiers, CLI transition
validation, exact R6 display-list serialization/clipping/draw order, the five
R8 software effects with determinism, pairwise distinct pixels, clamping,
overflow, R9 effect-command export for all five kinds with integer parameters,
draw order, bounds, R6 fixture invariance, and unchanged V0.1 golden digests.
`cargo test --offline` (111 tests) is green with fixed golden digests and zero
warnings. The cross-repo Metal byte-equality proof lives in `opurom-gpu`.

Not implemented / not measured — later rounds: a Metal/Vulkan/ANGLE GPU raster
or hardware-accelerated presentation inside this crate (the R6 Metal compute
path is cross-repo and headless); any fps claim, including a real-time 120fps
claim (there is no device matrix and the software raster has not been profiled);
real gesture hardware or an OS gesture recognizer; runtime device profiling in
the renderer (`opurom profile` remains the measured source); real image
decoding; real font shaping (text is a 5x7 bitmap font by design, so no Unicode
shaping, no hinting and no subpixel positioning; the SVG `<text>` is a generic
monospace approximation); OS screen-reader integration (the R3 tree is data
only: no ARIA/AT-SPI bridge, no focus order, no hit testing and no live
announcements); DPI scaling (`fontScale` scales logical pixels, not physical
DPI); native widget embedding; hardware presentation, GPU text, Vulkan/ANGLE/D3D
backends and real lens refraction (R10+). The R9 GPU effect path is a headless
cross-repo compute backend, not a presenter inside this crate. The R2 `glass: true` prop remains a colour blend; the
R8 `effects` glass path is a real CPU framebuffer blur, not refraction. Bitmap
glyphs are blocky by design. PPM drops alpha because all palette colours are
opaque. The SVG backend and R5 animation compositor are software backends, NOT
Metal/Vulkan/ANGLE GPU backends.

## Gaps / known limits

- `shortcut: 5x7 ASCII-only bitmap font, upgrade to a shaped font atlas when a GPU glyph path exists`.
- `shortcut: Image renders a deterministic colour placeholder, upgrade when resource loading exists`.
- `shortcut: dirty rects are capped at 64 (fallback: one full-frame rect), refine with a tile quadtree when a real presenter exists`.
- `shortcut: the gradient is vertical only (two stops), upgrade to an angle/multi-stop gradient when the IR supports it`.
- `shortcut: R2 glass is a colour blend; R8 glass blurs a framebuffer snapshot, upgrade to compositor layers and real refraction in R10+`.
- `shortcut: the entry animation grows and fades the box only, upgrade to per-property keyframes`.
- `shortcut: R5 transitions composite whole CPU frames with nearest-neighbour scale, upgrade to compositor layers with a real presenter`.
- `shortcut: gesture input is integer L1 deltas, upgrade when real hardware gesture events exist`.
- `shortcut: particles are fixed-seed 2x2 overlays, upgrade when a GPU particle path exists`.
- `shortcut: quality is a caller-supplied budget policy, upgrade when profile feedback is wired into the host`.
- `shortcut: the accessibility tree is layout-derived data, upgrade to a platform bridge when a native adapter exists`.
- `shortcut: SVG <text> uses a generic monospace face and built-in metrics, upgrade to real shaping when a font atlas exists`.
- `shortcut: SVG has no external crate and no image/font decoding; it is a software vector snapshot, not a GPU backend`.
- No cross-repo runtime link: the adapter crate's `FrameView` is matched by layout,
  not shared by type. That is intentional per `ADAPTER_CONTRACT.md`.
