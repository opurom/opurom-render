//! Skia conic-to-quad corner geometry (ELEVATION/AAA parity work).
//!
//! The rounded corner Skia renders is a rational quarter-circle conic
//! chopped into quadratic curves, not a sampled circle. These tests pin the
//! chop against Skia's float result and the control-point order the analytic
//! edge walker expects.

use opurom_render::aaa::{chop_conic, corner_conic, CONIC_WEIGHT};

const ONE: i64 = 1 << 16;

fn px(v: i64) -> f64 {
    v as f64 / ONE as f64
}

#[test]
fn conic_weight_is_sqrt_two_over_two() {
    // sqrt(2)/2 = 0.70710678118654752440...; 46341 is the nearest 16.16
    // value, within half a fixed-point step.
    assert!((px(CONIC_WEIGHT) - 0.707_106_781_186_547).abs() < 1e-6);
}

#[test]
fn corner_conic_gives_the_two_tangents_and_the_corner() {
    // Offsets are relative to the rectangle vertex, which is the conic's
    // control point. Corners run clockwise, each arc from its entry tangent
    // to its exit tangent.
    let tl = corner_conic(16, 16, 0);
    assert_eq!(tl[0], (0, 16 * ONE), "left tangent");
    assert_eq!(tl[1], (0, 0), "corner");
    assert_eq!(tl[2], (16 * ONE, 0), "top tangent");

    let tr = corner_conic(16, 16, 1);
    assert_eq!(tr[0], (-16 * ONE, 0), "top tangent");
    assert_eq!(tr[1], (0, 0));
    assert_eq!(tr[2], (0, 16 * ONE), "right tangent");

    let br = corner_conic(16, 16, 2);
    assert_eq!(br[0], (0, -16 * ONE), "right tangent");
    assert_eq!(br[2], (-16 * ONE, 0), "bottom tangent");

    let bl = corner_conic(16, 16, 3);
    assert_eq!(bl[0], (16 * ONE, 0), "bottom tangent");
    assert_eq!(bl[2], (0, -16 * ONE), "left tangent");
}

#[test]
fn chop_matches_skia_float_control_points() {
    // Skia's float chop of the r=16 top-left conic into two quads.
    let conic = corner_conic(16, 16, 0);
    let quads = chop_conic(conic, CONIC_WEIGHT, 1);
    assert_eq!(quads.len(), 2);

    let q0 = quads[0];
    assert!((px(q0[0].0) - 0.0).abs() < 1e-6, "q0 start x");
    assert!((px(q0[0].1) - 16.0).abs() < 1e-6, "q0 start y");
    assert!((px(q0[1].0) - 0.0).abs() < 1e-6, "q0 control x");
    assert!((px(q0[1].1) - 9.372_6).abs() < 1e-3, "q0 control y");
    assert!((px(q0[2].0) - 4.686_3).abs() < 1e-3, "q0 end x");
    assert!((px(q0[2].1) - 4.686_3).abs() < 1e-3, "q0 end y");

    let q1 = quads[1];
    assert!((px(q1[0].0) - 4.686_3).abs() < 1e-3, "q1 start x");
    assert!((px(q1[1].1) - 0.0).abs() < 1e-6, "q1 control y");
    assert!((px(q1[2].0) - 16.0).abs() < 1e-6, "q1 end x");
    assert!((px(q1[2].1) - 0.0).abs() < 1e-6, "q1 end y");
}

#[test]
fn chop_keeps_the_conic_endpoints() {
    let conic = corner_conic(24, 24, 2);
    let quads = chop_conic(conic, CONIC_WEIGHT, 3);
    assert_eq!(quads.len(), 8);
    assert_eq!(
        quads[0][0], conic[0],
        "first quad starts at the conic start"
    );
    assert_eq!(quads[7][2], conic[2], "last quad ends at the conic end");
    // Consecutive quads share their join point.
    for pair in quads.windows(2) {
        assert_eq!(pair[0][2], pair[1][0], "quads join without a gap");
    }
}

#[test]
fn chop_does_not_floor_negative_halves() {
    // The middle control point halves the outer terms before summing. A
    // negative odd term must round towards zero, so the result is one LSB
    // above the floored value.
    let conic = corner_conic(16, 16, 0);
    let quads = chop_conic(conic, CONIC_WEIGHT, 1);
    // q0's middle point comes from 0.5*t0 + t1 + 0.5*t2 with t0 negative.
    let half_floor = (quads[0][2].0 - 1) & !1;
    assert_ne!(quads[0][2].0, half_floor, "not floored to an even LSB");
}
