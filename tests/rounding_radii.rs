//! Integer Impeller corner geometry: radius normalization and the unit-circle
//! containment rule that the analytic corner coverage builds on.
//!
//! Sources: impeller/geometry/rounding_radii.cc:26 (Scaled, AdjustScale :15),
//! impeller/geometry/round_rect.cc:30 (CornerContains, Contains :73).

use opurom_render::geometry::{corner_contains, round_rect_contains, CornerRadii, RoundingRadii};

fn pair_ok(r: RoundingRadii, w: i64, h: i64) -> bool {
    r.top_left.x + r.top_right.x <= w
        && r.bottom_left.x + r.bottom_right.x <= w
        && r.top_left.y + r.bottom_left.y <= h
        && r.top_right.y + r.bottom_right.y <= h
}

#[test]
fn empty_bounds_or_flat_radii_go_to_zero() {
    assert_eq!(
        RoundingRadii::all(8).scaled(0, 10),
        RoundingRadii::default()
    );
    assert_eq!(
        RoundingRadii::all(8).scaled(10, 0),
        RoundingRadii::default()
    );
    let flat = RoundingRadii {
        top_left: CornerRadii { x: 5, y: 0 },
        ..RoundingRadii::default()
    };
    assert_eq!(flat.scaled(40, 40), RoundingRadii::default());
    let negative = RoundingRadii::all(-3);
    assert_eq!(negative.scaled(40, 40), RoundingRadii::default());
}

#[test]
fn radius_one_and_near_circle_are_not_scaled() {
    let one = RoundingRadii::all(1).scaled(40, 40);
    assert_eq!(one, RoundingRadii::all(1));
    // Radius exactly half the side length: adjacent sums equal the side, so
    // Impeller's ">" test leaves them untouched.
    let circle = RoundingRadii::all(50).scaled(100, 100);
    assert_eq!(circle, RoundingRadii::all(50));
    assert!(pair_ok(circle, 100, 100));
}

#[test]
fn radii_exceeding_an_edge_shrink_by_one_global_factor() {
    // 8 + 8 > 10, so the whole set scales by 10/16.
    let scaled = RoundingRadii::all(8).scaled(10, 40);
    assert!(pair_ok(scaled, 10, 40), "pair sums must fit");
    assert!(
        scaled.top_left.x < 8 && scaled.top_left.y < 8,
        "must shrink both axes"
    );
    assert_eq!(scaled.top_left, scaled.top_right, "global scale is uniform");
    assert_eq!(scaled.top_left, scaled.bottom_left);
    // Floor scaling still fills the width to the last integer pixel.
    assert!(scaled.top_left.x + scaled.top_right.x >= 8);
}

#[test]
fn asymmetric_corners_share_one_scale() {
    let r = RoundingRadii {
        top_left: CornerRadii { x: 9, y: 4 },
        top_right: CornerRadii { x: 2, y: 3 },
        bottom_left: CornerRadii { x: 1, y: 2 },
        bottom_right: CornerRadii { x: 1, y: 1 },
    };
    let scaled = r.scaled(10, 10);
    assert!(pair_ok(scaled, 10, 10));
    assert!(scaled.top_left.x < r.top_left.x, "the large corner shrinks");
    assert!(scaled.bottom_right.x <= r.bottom_right.x, "no radius grows");
    assert!(
        scaled.top_left.y < r.top_left.y,
        "the global factor touches y too"
    );
}

#[test]
fn elliptical_radii_are_constrained_per_axis() {
    // x pair 30+30 > 40 forces 40/60; y pair 6+6 > 10 forces 10/12. The global
    // factor is the smaller one, applied to BOTH components.
    let r = RoundingRadii {
        top_left: CornerRadii { x: 30, y: 6 },
        top_right: CornerRadii { x: 30, y: 6 },
        bottom_left: CornerRadii { x: 30, y: 6 },
        bottom_right: CornerRadii { x: 30, y: 6 },
    };
    let s = r.scaled(40, 10);
    // Floor scaling: 30 * floor(40/60) = 19 (integer conservative), not 20.
    assert_eq!(s.top_left.x, 19);
    assert_eq!(s.top_left.y, 3);
    assert!(pair_ok(s, 40, 10));
    assert_ne!(s.top_left.x, s.top_left.y, "x and y need not match");
}

#[test]
fn unit_circle_containment_matches_impeller_geometry() {
    let r = CornerRadii { x: 40, y: 40 };
    // Quarter circle of radius 40 centred at (40,40) in the upper-left corner.
    assert!(!corner_contains(0, 0, (0, 0), (-1, -1), r)); // arc corner is outside
    assert!(!corner_contains(10, 10, (0, 0), (-1, -1), r)); // distance 42.4 > 40
    assert!(corner_contains(15, 15, (0, 0), (-1, -1), r)); // distance 35.4 < 40
                                                           // A point outside the curved quadrant is allowed through to bounds tests.
    assert!(corner_contains(60, 60, (0, 0), (-1, -1), r));
    // Flat corners never reject.
    assert!(corner_contains(
        0,
        0,
        (0, 0),
        (-1, -1),
        CornerRadii { x: 0, y: 5 }
    ));
}

#[test]
fn elliptical_corner_uses_normalized_unit_circle() {
    // Arc centre (40,20), rx=40 ry=20. (10,5) has normalized distance
    // sqrt(0.75^2 + 0.75^2) > 1 -> outside; (12,7) is inside.
    let r = CornerRadii { x: 40, y: 20 };
    assert!(corner_contains(12, 7, (0, 0), (-1, -1), r));
    assert!(!corner_contains(10, 5, (0, 0), (-1, -1), r));
}

#[test]
fn round_rect_contains_checks_bounds_and_all_four_corners() {
    let r = RoundingRadii::all(20).scaled(100, 60);
    assert!(round_rect_contains(50, 30, 0, 0, 100, 60, r));
    assert!(!round_rect_contains(-1, 30, 0, 0, 100, 60, r));
    assert!(!round_rect_contains(0, 0, 0, 0, 100, 60, r)); // corner is outside
    assert!(round_rect_contains(30, 10, 0, 0, 100, 60, r));
}
