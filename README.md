# opurom-render

Independent Rust crate: IR v0.1 → adaptive layout → deterministic RGBA8 software
raster → incremental dirty rects. This folder is its own git repository; it has no
source dependency on any other OPUROM layer, only on the frozen IR/ABI contracts
(`IR_SCHEMA.md`, `ADAPTER_CONTRACT.md`) and local `FrameView` shape compatibility.

## Verify

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd opurom-render
cargo test
```

`cargo test` runs 17 tests (unit + integration). All pass. Only `serde_json` is used;
it is pinned to the locally cached `=1.0.154`, so the build needs no network.

CLI (integration surface for `opurom-cli` and the e2e suite):

```sh
cargo build
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm
# digest=8cc3021fa80749b5
# dirty=1
./target/debug/opurom-render ../opurom-spec/counter.opuir.json 800 600 out.ppm --state count=7
# digest=e7aad51c98d9cf21
```

Usage: `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...]`.
`--state` repeats; values are parsed against the declared state type. Output is a
P6 PPM (RGB; the opaque alpha byte is dropped). Stdout has exactly two lines:
`digest=<16 lowercase hex>` (FNV-1a 64 over the **RGBA** frame bytes) and
`dirty=<rect count>`. Malformed IR or a bad viewport is a readable stderr
diagnostic with exit code 1, never a panic. A fresh process always reports
`dirty=1` (first frame = whole surface); unchanged state within one `Renderer`
yields zero dirty rects.

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
  node ids, the eight v0.1 kinds, known props, event actions (targets/types) and the
  theme. Unknown props are ignored. Errors are `IrError(String)` values.
- `src/layout.rs` — pure integer layout. Breakpoints on **logical px**: compact
  `<600`, medium `<900`, expanded `>=900`. `Row` collapses to a vertical stack in
  compact. Children share the main axis in proportion to their natural size (fluid
  sizing, i128 division with a deterministic remainder), honouring explicit
  `width`/`height`. `gap`, `padding`, `align`, `radius` and `color` are resolved here.
- `src/font.rs` — built-in 5x7 bitmap font, printable ASCII `0x20..=0x7E` (A–Z,
  a–z, 0–9, punctuation). Unsupported codepoints consume a blank advance.
- `src/raster.rs` — filled rounded rectangles (integer quarter-circle test), 1 px
  borders, scaled glyph blocks, opaque RGBA with integer alpha blending.
- `src/lib.rs` — `Rgba`, `StateView`, `Renderer`, `Frame`, `FrameView`, inline FNV-1a
  digest, and row/column dirty-rect diffing.
- `src/main.rs` — the CLI described above.

## Determinism guarantee

Same IR + same viewport + same state ⇒ byte-identical RGBA pixels on every host.
Mechanisms: all layout and raster arithmetic is integer (`i64`/`i128`, no float
ordering, no `f32` rounding); state lives in a `BTreeMap`; events are visited in
sorted key order; `theme.mode == "system"` resolves to light instead of querying the
host; no time, randomness, threads, environment variables or resource I/O affect the
frame (`Image` uses a stable hash of `src` as a placeholder colour). Golden test:
800x600, `counter.opuir.json`, default state ⇒ `digest=8cc3021fa80749b5`. A second
test renders the same IR with two fresh renderers at two viewports and asserts
byte equality within each viewport.

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

## Honest status (round 1)

Verified now: IR parsing/validation, adaptive breakpoints + Row collapse + fluid
sizing, all eight node kinds, light/dark theme + accent, built-in 5x7 font,
deterministic RGBA8 software raster, incremental dirty-rect diffing with coverage
and bound tests, interpolation from `StateView`, `FrameView` shape, and the PPM CLI.
`cargo test` is green (17 tests) with a fixed golden digest.

Not implemented / not measured — later rounds: Metal, Vulkan and ANGLE GPU
backends; any hardware-accelerated presentation; the 120 fps target (no measured
fps is claimed here — there is no GPU backend or device matrix yet); particles,
glass/blur and other effects; animation, subpixel glyph hinting, Unicode shaping,
real image decoding, DPI scaling and native widget embedding. Bitmap glyphs are
blocky by design. PPM drops alpha because all palette colours are opaque.

## Gaps / known limits

- `shortcut: 5x7 ASCII-only bitmap font, upgrade to a shaped font atlas when the GPU backend lands`.
- `shortcut: Image renders a deterministic colour placeholder, upgrade when resource loading exists`.
- `shortcut: dirty rects are capped at 64 (fallback: one full-frame rect), refine with a tile quadtree when a real presenter exists`.
- No cross-repo runtime link: the adapter crate's `FrameView` is matched by layout,
  not shared by type. That is intentional per `ADAPTER_CONTRACT.md`.
