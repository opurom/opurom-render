# Visual Quality Contract

Frozen contract for renderer targets. The IR remains the single source of
truth; targets lower visual settings from IR plus this profile. No target
imports another target's source.

## Wire fields

The existing display-list header is unchanged for compatibility:

```json
{"version":1,"width":320,"height":240,"background":[247,248,250,255],"commands":[]}
```

`QualityProfile` values are stable integers:

- `compat = 0`: historical byte-stable behavior, fixed font metrics, bitmap
  font fallback, no app-level motion, no edge coverage, byte comparison.
- `product = 1`: analytic edge coverage, proportional font metrics, system
  font overlay, effects enabled, app-level motion, DPR-aware raster,
  perceptual comparison.

Targets receive the profile through their own ABI or CLI flag lowered from
the same profile name. They must not add or remove display-list fields to
represent it.

## Renderer API

- `Renderer::try_new` and `Renderer::new` start in Compat.
- `Renderer::set_quality(QualityProfile::Product)` enables Product.
- `Renderer::quality()` reports the active profile.
- Legacy `set_edge_aa(true)` keeps its historical boolean behavior.

## Determinism

- Compat: identical IR, viewport and profile produce identical bytes.
- Product: identical IR, viewport, profile and DPR produce stable perceptual
  output; cross-target RGBA bit-exactness is not required.

## CLI contract

The canonical CLI flag is `--quality compat|product`. Legacy `--edge-aa` and
`--font-metrics fixed|proportional` map into the same profile resolver without
changing frozen compat digests. Product defaults apply to user-facing
build/dev/serve/export commands; golden tests explicitly select compat.
