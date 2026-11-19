//! Material 3 duration/easing tokens from Flutter framework 3.50.0
//! (`ab2912035e`), `packages/flutter/lib/src/material/motion.dart:28/60/192`.
//! The two new names are opt-in; all previously pinned motion digests stay
//! byte-identical.

use opurom_render::{
    animate_progress, default_duration_ms, parse_ir, Curve, M3_MEDIUM1_MS, M3_SHORT1_MS, ONE,
};

#[test]
fn m3_duration_names_round_trip_and_map_to_flutter_constants() {
    assert_eq!(Curve::parse("m3-short1"), Some(Curve::M3Short1));
    assert_eq!(Curve::parse("m3-medium1"), Some(Curve::M3Medium1));
    assert_eq!(Curve::M3Short1.name(), "m3-short1");
    assert_eq!(Curve::M3Medium1.name(), "m3-medium1");
    assert_eq!(M3_SHORT1_MS, 50);
    assert_eq!(M3_MEDIUM1_MS, 250);
    assert_eq!(default_duration_ms(Curve::M3Short1), M3_SHORT1_MS);
    assert_eq!(default_duration_ms(Curve::M3Medium1), M3_MEDIUM1_MS);
}

#[test]
fn standard_easing_is_monotonic_with_exact_endpoints() {
    for curve in [Curve::M3Short1, Curve::M3Medium1] {
        let duration = default_duration_ms(curve);
        let mut previous = -1;
        for i in 0..=64 {
            let value = animate_progress(curve, duration * i / 64, duration, 0);
            assert!(value >= previous, "{curve:?} must not decrease");
            assert!((0..=ONE).contains(&value));
            previous = value;
        }
        assert_eq!(animate_progress(curve, 0, duration, 0), 0);
        assert_eq!(animate_progress(curve, duration, duration, 0), ONE);
        assert_eq!(animate_progress(curve, duration + 500, duration, 0), ONE);
    }
}

#[test]
fn standard_easing_accelerates_then_decelerates() {
    // Cubic(0.2,0,0,1) is slow at the start and slow at the end; the middle
    // third must already have covered more progress than elapsed time.
    let p = |ms| animate_progress(Curve::M3Medium1, ms, M3_MEDIUM1_MS, 0);
    assert!(p(25) < 1024 / 4, "first quarter stays slow");
    assert!(p(125) > 1024 / 2, "middle is ahead of linear time");
    assert!(p(225) > 1024 * 9 / 10, "last quarter nearly settled");
}

#[test]
fn both_names_are_accepted_by_the_ir_parser() {
    for name in ["m3-short1", "m3-medium1"] {
        let doc = r##"{"ir":"0.1.0","app":"M3Motion","state":[],"tree":{"id":0,"kind":"Text","props":{"value":"x","animate":"@NAME@"},"children":[],"events":{}},"theme":{"mode":"light","accent":"#097FEB","radius":0}}"##.replace("@NAME@", name);
        assert!(parse_ir(&doc).is_ok(), "{name} must parse");
    }
}
