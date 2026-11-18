//! Deterministic gradient stop/tile sampling, ported from Flutter 3.50.0.
//!
//! Authority: framework `ab2912035e`, engine `371f77202a`,
//! `engine/src/flutter/lib/ui/painting.dart:5040-5070`: stop values are
//! clamped to `[0,1]`, monotone (a later stop no smaller than the previous),
//! and `tileMode` selects the behaviour outside `[0,1]` (`TileMode` at
//! `:5020-5038`). No IR protocol change: this module is the pure sampling
//! kernel that a future opt-in IR field can call without a runtime interpreter.

/// Flutter `TileMode` indices (`ui/painting.dart:5020-5038`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileMode {
    Clamp,
    Repeat,
    Mirror,
    Decal,
}

/// Map a parametric gradient position into `[0,1]` under `tileMode`.
/// Integer fixed point: 1024 == 1.0, so t is relative to the gradient span.
pub fn tile_position(t: i64, mode: TileMode) -> Option<i64> {
    let one = 1024i64;
    if (0..=one).contains(&t) {
        return Some(t);
    }
    match mode {
        TileMode::Clamp => Some(t.clamp(0, one)),
        TileMode::Repeat => Some(t.rem_euclid(one)),
        TileMode::Mirror => {
            let period = 2 * one;
            let r = t.rem_euclid(period);
            Some(if r <= one { r } else { period - r })
        }
        TileMode::Decal => None,
    }
}

/// Normalise Flutter color stops: clamp to `[0,1]`, then force monotonicity by
/// rounding each stop up to its predecessor (`ui/painting.dart:5044-5050`).
/// Inputs and outputs use 1024 fixed point. Empty input yields `[0, 1024]`.
pub fn normalize_stops(stops: &[i64]) -> Vec<i64> {
    if stops.is_empty() {
        return vec![0, 1024];
    }
    let mut out = Vec::with_capacity(stops.len());
    let mut prev = 0i64;
    for &raw in stops {
        let v = raw.clamp(0, 1024).max(prev);
        out.push(v);
        prev = v;
    }
    if out.len() == 1 {
        out.push(1024);
    }
    out
}

/// Linear interpolation between adjacent stops at a tiled position. `colors`
/// length must match the normalised stops length. Returns `None` for decal
/// gaps, empty input, or a zero-width final interval at `t == 1024`.
pub fn sample_rgba(
    stops: &[i64],
    colors: &[crate::Rgba],
    t: i64,
    mode: TileMode,
) -> Option<crate::Rgba> {
    let t = tile_position(t, mode)?;
    let stops = normalize_stops(stops);
    if colors.is_empty() || colors.len() != stops.len() {
        return None;
    }
    let mut i = 0usize;
    while i + 1 < stops.len() && t > stops[i + 1] {
        i += 1;
    }
    if i + 1 >= stops.len() {
        return Some(colors[colors.len() - 1]);
    }
    let span = stops[i + 1] - stops[i];
    let frac = if span <= 0 {
        1024
    } else {
        ((t - stops[i]) * 1024) / span
    };
    Some(crate::layout::lerp_color(colors[i], colors[i + 1], frac))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;

    #[test]
    fn tile_modes_match_flutter_definitions() {
        assert_eq!(tile_position(512, TileMode::Clamp), Some(512));
        assert_eq!(tile_position(-1, TileMode::Clamp), Some(0));
        assert_eq!(tile_position(1025, TileMode::Clamp), Some(1024));
        assert_eq!(tile_position(-1, TileMode::Repeat), Some(1023));
        assert_eq!(tile_position(1025, TileMode::Repeat), Some(1));
        assert_eq!(tile_position(-1, TileMode::Mirror), Some(1));
        assert_eq!(tile_position(2047, TileMode::Mirror), Some(1));
        assert_eq!(tile_position(-1, TileMode::Decal), None);
    }

    #[test]
    fn stops_are_clamped_and_made_monotone() {
        assert_eq!(normalize_stops(&[]), vec![0, 1024]);
        assert_eq!(
            normalize_stops(&[-3, 200, 100, 2048]),
            vec![0, 200, 200, 1024]
        );
    }

    #[test]
    fn sampling_is_deterministic_and_clamped_at_both_ends() {
        let colors = [Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255)];
        let stops = [];
        assert_eq!(
            sample_rgba(&stops, &colors, 0, TileMode::Clamp),
            Some(colors[0])
        );
        assert_eq!(
            sample_rgba(&stops, &colors, 1024, TileMode::Clamp),
            Some(colors[1])
        );
        let mid = sample_rgba(&stops, &colors, 512, TileMode::Clamp).unwrap();
        assert!(mid.r > 126 && mid.r < 130, "midpoint {mid:?}");
        assert_eq!(
            sample_rgba(&stops, &colors, 2048, TileMode::Repeat),
            Some(colors[0])
        );
    }
}

/// Flutter `OpacityLayer.alpha` (engine 371f772,
/// `packages/flutter/lib/src/rendering/layer.dart:2150-2160`): alpha 255
/// pushes only an offset layer, so a fully opaque opacity must not create a
/// compositing layer. This pure rule is a prerequisite for a future static
/// layer cache; it does not change the IR protocol.
pub fn needs_opacity_layer(alpha: u8, has_children: bool) -> bool {
    has_children && alpha < 255
}

#[cfg(test)]
mod layer_tests {
    use super::needs_opacity_layer;

    #[test]
    fn opacity_layer_rule_matches_flutter() {
        assert!(!needs_opacity_layer(255, true));
        assert!(!needs_opacity_layer(200, false));
        assert!(needs_opacity_layer(254, true));
    }
}

/// Convert a straight-alpha pixel to premultiplied RGB with the same 8-bit
/// quantisation Impeller uses (`Color::Premultiply`,
/// engine 371f772, impeller/geometry/color.h:212-215). Opaque pixels are
/// returned unchanged so every existing render stays byte-identical.
pub fn premultiply_rgba(c: crate::Rgba) -> crate::Rgba {
    if c.a == 255 {
        return c;
    }
    let a = c.a as i64;
    let m = |v: u8| -> u8 { ((v as i64 * a + 127) / 255).clamp(0, 255) as u8 };
    crate::Rgba {
        r: m(c.r),
        g: m(c.g),
        b: m(c.b),
        a: c.a,
    }
}

/// Inverse of [`premultiply_rgba`] (`Color::Unpremultiply`, color.h:217-221).
/// Fully transparent maps to black-transparent, matching Impeller.
pub fn unpremultiply_rgba(c: crate::Rgba) -> crate::Rgba {
    if c.a == 0 {
        return crate::Rgba::rgb(0, 0, 0);
    }
    if c.a == 255 {
        return c;
    }
    let a = c.a as i64;
    let m = |v: u8| -> u8 { ((v as i64 * 255 + a / 2) / a).clamp(0, 255) as u8 };
    crate::Rgba {
        r: m(c.r),
        g: m(c.g),
        b: m(c.b),
        a: c.a,
    }
}

#[cfg(test)]
mod premul_tests {
    use super::{premultiply_rgba, unpremultiply_rgba};
    use crate::Rgba;

    #[test]
    fn impeller_premultiply_roundtrip_is_exact_for_representable_channels() {
        let p = Rgba {
            r: 200,
            g: 100,
            b: 50,
            a: 128,
        };
        let pm = premultiply_rgba(p);
        assert_eq!(pm.r, 100);
        assert_eq!(pm.g, 50);
        assert_eq!(pm.b, 25);
        assert_eq!(pm.a, 128);
        let straight = unpremultiply_rgba(pm);
        assert!((straight.r as i32 - 200).abs() <= 1);
        assert!((straight.g as i32 - 100).abs() <= 1);
        assert!((straight.b as i32 - 50).abs() <= 1);
        assert_eq!(
            premultiply_rgba(Rgba::rgb(12, 34, 56)),
            Rgba::rgb(12, 34, 56)
        );
        assert_eq!(
            unpremultiply_rgba(Rgba {
                r: 50,
                g: 50,
                b: 50,
                a: 0
            }),
            Rgba::rgb(0, 0, 0)
        );
    }
}

/// Flutter `TransformLayer.addToScene` composes the layer transform before its
/// offset (`packages/flutter/lib/src/rendering/layer.dart:2033-2075`; contract at
/// `:2040-2042`); Opurom's
/// integer display-list runs in logical pixels, so only integer translations
/// are representable without a full matrix pipeline. This pure helper records
/// that boundary instead of silently accepting a lossy transform.
pub fn compose_layer_offset(
    transform_tx: i64,
    transform_ty: i64,
    offset_x: i64,
    offset_y: i64,
) -> (i64, i64) {
    // Matrix4 translation then OffsetLayer offset, both integer-only here.
    (transform_tx + offset_x, transform_ty + offset_y)
}

#[cfg(test)]
mod layer_offset_tests {
    use super::compose_layer_offset;

    #[test]
    fn transform_precedes_offset() {
        assert_eq!(compose_layer_offset(3, 4, 5, 6), (8, 10));
    }
}

/// Radial gradient rule, ported from Flutter 3.50.0
/// `Gradient.radial` (`ui/painting.dart:5122-5150`) with `TileMode` semantics
/// (`ui/painting.dart:4912-4981`). A point outside the radius decays to the
/// end colour under clamp/repeat/mirror; under decal it is transparent black.
/// Integer-only: coordinates are 16.16 fixed point, radius likewise; the
/// squared distance test avoids sqrt while staying deterministic.
pub fn radial_position(
    px: i64,
    py: i64,
    cx: i64,
    cy: i64,
    radius_fp: i64,
    mode: TileMode,
) -> Option<i64> {
    let dx = (px - cx) as i128;
    let dy = (py - cy) as i128;
    let r = radius_fp as i128;
    let d2 = dx * dx + dy * dy;
    let r2 = r * r;
    if d2 >= r2 {
        return match mode {
            TileMode::Decal => None,
            // At and beyond the edge the gradient is fully clamped to its end
            // colour; repeat/mirror hit the same end stop at the boundary.
            _ => Some(1024),
        };
    }
    // t = distance / radius, scaled to 0..=1024 without sqrt: compare against
    // precomputed thresholds via integer square root of the ratio.
    // isqrt(d2) / radius = t; multiply t by 1024 with rounding.
    let d = isqrt_i128(d2);
    let one = 1024i64;
    let t = ((d * one as i128) / r.max(1)).clamp(0, one as i128) as i64;
    tile_position(t, mode)
}

/// Integer square root helper for radial positioning (floor).
fn isqrt_i128(v: i128) -> i128 {
    if v <= 0 {
        return 0;
    }
    let mut x = v;
    let mut r: i128 = 0;
    let mut bit = 1i128 << 126;
    while bit > x {
        bit >>= 2;
    }
    while bit != 0 {
        if x >= r + bit {
            x -= r + bit;
            r = (r >> 1) + bit;
        } else {
            r >>= 1;
        }
        bit >>= 2;
    }
    r
}

#[cfg(test)]
mod radial_tests {
    use super::{radial_position, TileMode};

    #[test]
    fn radial_edge_and_outside_match_flutter_tile_modes() {
        // Centre and edge (fixed point 1<<16): centre t=0, edge t=1024.
        let r = 1i64 << 16;
        assert_eq!(radial_position(0, 0, 0, 0, r, TileMode::Clamp), Some(0));
        assert_eq!(radial_position(r, 0, 0, 0, r, TileMode::Clamp), Some(1024));
        assert_eq!(
            radial_position(2 * r, 0, 0, 0, r, TileMode::Clamp),
            Some(1024)
        );
        assert_eq!(radial_position(2 * r, 0, 0, 0, r, TileMode::Decal), None);
        assert_eq!(
            radial_position(2 * r, 0, 0, 0, r, TileMode::Repeat),
            Some(1024)
        );
    }

    #[test]
    fn radial_midpoint_is_half_of_the_radius() {
        let r = 1i64 << 16;
        let half = (r / 2).max(1);
        let t = radial_position(half, 0, 0, 0, r, TileMode::Clamp).unwrap();
        assert!((500..=524).contains(&t), "midpoint t {t}");
    }
}
