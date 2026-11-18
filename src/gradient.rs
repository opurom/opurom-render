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
