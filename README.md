# opurom-render

Independent Rust crate: IR v0.1 → adaptive layout → deterministic RGBA8 software
raster → incremental dirty rects. This folder is its own git repository; it has no
source dependency on any other OPUROM layer, only on the frozen IR/ABI contracts
(`IR_SCHEMA.md`, `ADAPTER_CONTRACT.md`) and local `FrameView` shape compatibility.

## Verify

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd opurom-render
cargo fmt --check
cargo test
```

`cargo test` runs 57 tests (unit + integration). All pass, zero warnings. Only
`serde_json` is used; it is pinned to the locally cached `=1.0.154`, so the build
needs no network.

CLI (integration surface for `opurom-cli` and the e2e suite):

```sh
cargo build
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm
# digest=8cc3021fa80749b5
# dirty=1
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --state count=7
# digest=e7aad51c98d9cf21
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --a11y tree.json
# digest=8cc3021fa80749b5, dirty=1 (stdout unchanged)
```

Usage: `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick ms] [--a11y out.json]`.
`--state` repeats; values are parsed against the declared state type. `--tick`
sets the animation clock position in milliseconds (default: the settled end
state, so a v0.1 IR renders exactly as in round 1). Output is a P6 PPM (RGB; the
opaque alpha byte is dropped). Stdout has exactly two lines:
`digest=<16 lowercase hex>` (FNV-1a 64 over the **RGBA** frame bytes) and
`dirty=<rect count>`. Malformed IR or a bad viewport is a readable stderr
diagnostic with exit code 1, never a panic. A fresh process always reports
`dirty=1` (first frame = whole surface); unchanged state within one `Renderer`
yields zero dirty rects. `--a11y <out.json>` writes a separate deterministic
JSON accessibility tree (see below) and does not add stdout lines.

## Architecture

```
serde_json::Value ── parse_ir (validate) ──▶ Ir { state, tree: Node, theme }
                                              │
                          StateView (flat BTreeMap, {name} interpolation)
                                              │
                     layout(Ir, StateView, viewport) ──▶ Vec<LaidOut>  (pre-order)
                                              │
              raster::Canvas (rounded fills/borders + 5x7 glyphs) ──▶ Vec<u8> RGBA8
                                              │
              Renderer::render diff vs previous buffer ──▶ Frame { dirty }
                                              │
                              Frame::view() ──▶ FrameView<'_> for adapters
```

- `src/ir.rs` — strictly validates `ir == "0.1"`, state decls/types/inits, pre-order
  node ids, the 24 node kinds, known props, event actions (targets/types) and the
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
  integer alpha blending.
- `src/anim.rs` — the fixed-timestep animation clock and the spring/ease curves.
- `src/lib.rs` — `Rgba`, `StateView`, `Renderer`, `Frame`, `FrameView`, inline FNV-1a
  digest, and row/column dirty-rect diffing.
- `src/a11y.rs` — R3 pre-order accessibility tree and its deterministic JSON serializer.
- `src/main.rs` — the CLI described above.

## R2 features (IR_R2_ADDENDUM)

All R2 fields are **additive and opt-in**. A v0.1 IR file renders byte-identically
to round 1 — the three golden digests below are unchanged.

| Area | What the renderer does |
| --- | --- |
| Theme tokens | `theme.tokens.surface` / `.onSurface` / `.font` override the `mode` defaults; every token falls back independently. The `font` token is recorded but never selects a host font: text always uses the built-in bitmap font. |
| Style effects | `gradient` (two-stop vertical linear gradient, integer lerp), `glass` (surface blended with the accent), `shadow` (0..8, offset darkening), `opacity` (0..1, exact fixed point 0..1024), `border` (1 px colour), `radius` (rounded corners), `margin`. Effects apply to every kind that owns a box (Container, Card, Button, …). |
| Conditional visibility | The optional node field `when: "<boolState>"` hides the node **and its subtree** from layout and from the raster while the state is false. A `when` on a non-bool or undeclared state is a parse error. |
| Node kinds | All 24 kinds render: the eight v0.1 kinds plus `Card, Divider, Icon, Badge, Progress, Switch, Slider, Checkbox, Radio, Dialog, Tabs, List, Grid, Scroll, Avatar, Spinner`. `Grid` uses `columns` for a fluid grid, `Scroll` clips to `height`, `List` is a vertical stack, `Progress`/`Slider` draw a track plus fill, `Switch`/`Checkbox`/`Radio` draw a box plus indicator. An unknown kind degrades to the documented fallback (Container with children, else Text using label/value/title/name) instead of failing. |
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

## Determinism guarantee

Same IR + same viewport + same state + same animation tick ⇒ byte-identical RGBA
pixels on every host. Mechanisms: all layout and raster arithmetic is integer
(`i64`/`i128`, no float ordering, no `f32` rounding); state lives in a `BTreeMap`;
events are visited in sorted key order; `theme.mode == "system"` resolves to light
instead of querying the host; no time, randomness, threads, environment variables
or resource I/O affect the frame (`Image` uses a stable hash of `src` as a
placeholder colour); the animation clock only moves when the caller ticks it, so
two identical tick sequences produce identical digest sequences.

Golden digests (unchanged by R2 and R3):

| Fixture | Viewport | Digest |
| --- | --- | --- |
| `counter.opuir.json` | 320x240 | `c87bf023a6c01f9d` |
| `counter.opuir.json` | 800x600 | `8cc3021fa80749b5` |
| `counter.opuir.json`, `--state count=7` | 800x600 | `e7aad51c98d9cf21` |

`FrameView<'a> { width: u32, height: u32, pixels: &'a [u8] }` has the exact field
layout of `ADAPTER_CONTRACT.md`. It is a local struct: adapters do not gain a
compile-time dependency on this crate, and this crate does not import theirs.

## Where the GPU backend slots in (later round)

The pixel contract is fixed now, so a GPU backend is a **replacement for the raster
stage only**. `layout()` stays the source of `Vec<LaidOut>`; a Metal/Vulkan/ANGLE
backend would translate those boxes/glyph runs into draw calls and copy the result
into the same `Frame { width, height, pixels, dirty }`. `Frame::view()` already
matches the adapter ABI, so `present(FrameView)` is unchanged. A backend must pass
the same golden digest to be considered conformant; the headless software raster is
the conformance oracle. Dirty rects remain a property of the diff stage, not of the
backend.

## Honest status (round 3)

Verified now, by tests in this crate: IR parsing/validation, adaptive breakpoints +
Row collapse + fluid sizing, all 24 node kinds, theme tokens with per-token
fallback, the six canonical style effects, conditional `when` visibility with
bounded dirty rects, light/dark theme + accent, built-in 5x7 font, deterministic
RGBA8 software raster, incremental dirty-rect diffing with coverage and bound
tests, interpolation from `StateView`, `FrameView` shape, the PPM CLI, integer
`fontScale`, high-contrast rendering, the `theme.tokens.contrast` token, and a
deterministic pre-order accessibility tree with roles, labels, bounds and hidden
flags. `cargo test` is green with fixed golden digests and zero warnings.

Not implemented / not measured — later rounds: Metal, Vulkan and ANGLE GPU
backends; any hardware-accelerated presentation; the 120 fps target (**no measured
fps is claimed here** — there is no GPU backend or device matrix yet, and the
software raster has not been profiled on any device); particles; real image
decoding; real font shaping (text is a 5x7 bitmap font by design, so no
Unicode shaping, no hinting and no subpixel positioning); OS screen-reader
integration (the R3 tree is data only: no ARIA/AT-SPI bridge, no focus order,
no hit testing and no live announcements); DPI scaling (`fontScale` scales
logical pixels, not physical DPI); native widget embedding; blur behind glass
(the frosted fill is a colour blend, not a blur); GPU-side shadows. Bitmap
glyphs are blocky by design. PPM drops alpha because all palette colours are
opaque.

## Gaps / known limits

- `shortcut: 5x7 ASCII-only bitmap font, upgrade to a shaped font atlas when the GPU backend lands`.
- `shortcut: Image renders a deterministic colour placeholder, upgrade when resource loading exists`.
- `shortcut: dirty rects are capped at 64 (fallback: one full-frame rect), refine with a tile quadtree when a real presenter exists`.
- `shortcut: the gradient is vertical only (two stops), upgrade to an angle/multi-stop gradient with the GPU backend`.
- `shortcut: glass is a colour blend, not a real blur (a blur needs a render target)`.
- `shortcut: the entry animation grows and fades the box only, upgrade to per-property keyframes`.
- `shortcut: the accessibility tree is layout-derived data, upgrade to a platform bridge when a native adapter exists`.
- No cross-repo runtime link: the adapter crate's `FrameView` is matched by layout,
  not shared by type. That is intentional per `ADAPTER_CONTRACT.md`.
