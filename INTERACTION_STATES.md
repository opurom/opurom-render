# Interaction state matrix

The default UI reference requires every interactive component to have a
Default / Hover / Active / Focus / Disabled matrix. `opurom-render` implements
that matrix deterministically and adds the reference's variant/tone matrix.
All new props are opt-in: an IR without them renders byte-identically to the
frozen 0.1.0 goldens.

## Node props

- `state`: `default` (also the absent default), `hover`, `active`, `focus`, or
  `disabled`.
- `disabled`: boolean. It wins over `state` and over `--interactive-state`.
- `variant`: `primary`, `secondary`, `text`, `danger`, `ghost`, or `fab`.
- `tone`: `success`, `warning`, `error`, `info`, or `neutral`.

Unknown values are `IrError` diagnostics that name the prop, the offending
value, and the accepted list. `variant`/`tone` defaults never replace an
explicit `color` or `border` prop. A tone always wins on the border colour.

## Deterministic pixel rules

Integer channel transforms apply to the central laid-out palette (`fill`,
`border`, text, accent, gradient stops, effect colours and particle colours):

- Hover: `c' = c + (255 - c) * 3 / 16` per RGB channel (white tint).
- Active: `c' = c * 13 / 16` per RGB channel. A 1 px top/left inset stroke
  uses the styled accent darkened again by `3 / 4`.
- Focus: colours are unchanged. The theme's brand accent paints a 2 px ring in
  four strips strictly outside the node rect: `x-2..x-1`, `x+w..x+w+1`,
  `y-2..y-1`, and `y+h..y+h+1`. No pixel inside the rect is part of the ring.
- Disabled: integer luminance `l = (299r + 587g + 114b) / 1000`; each channel
  becomes `(c + 2l) / 3`, colour alpha becomes `a * 3 / 5`, and fixed-point
  opacity/alphas become `alpha * 3 / 5`. The framebuffer remains opaque after
  integer alpha blending.

All arithmetic above is integer division, so the result is host-independent.
The default theme accent `#097FEB` is unchanged.

## Variants and measured palette

These values are sampled from the supplied UI reference PNGs and are used only
as defaults for the new variant/tone props:

| Token | Value |
| --- | --- |
| primary / info | `#0A71FC` |
| cyan / accent | `#0DB2FF` |
| success | `#1FB14C` |
| warning | `#FD960E` |
| error | `#F62B35` |
| deep ink / neutral | `#132C5A` |
| white ink | `#FFFFFF` |

- `primary`: filled `#0A71FC`, white ink.
- `secondary`: transparent interior, 1 px `#0A71FC` outline, deep-ink text.
- `text`: transparent interior and no box, `#0A71FC` text.
- `danger`: filled `#F62B35`, white ink.
- `ghost`: white tinted `1/8` toward `#0DB2FF` using fixed-point integer
  blending, deep-ink text.
- `fab`: circular `#0DB2FF` fill, white ink, radius forced to half the shorter
  rect side.

A tone adds a 1 px border in its measured colour plus a 4 px dot of the same
colour, inset 2 px from the upper-right corner. `info` is `#0A71FC` and
`neutral` is `#132C5A`. Tone and variant colours pass through the state
transforms, so disabled primary and tone pixels are desaturated and reduced
in alpha.

## CLI

`--state name=value ...` remains the state-variable override. The separate
`--interactive-state <default|hover|active|focus|disabled>` flag applies one
visual state to every interactive node in the tree, including nested nodes,
for a state-matrix screenshot. An authored `disabled` prop still wins. The
flag adds no stdout lines: without other optional flags stdout remains exactly
`digest=<16 hex>` followed by `dirty=<count>`.

```sh
opurom-render matrix.opuir.json 800 600 matrix-hover.ppm --interactive-state hover
```
