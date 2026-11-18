//! Hit-target geometry pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `material/constants.dart:27`: `kMinInteractiveDimension` is 48 logical
//!   pixels. Opurom resolves it into the IR design domain as
//!   `MIN_HIT_SIZE`.
//! - `gestures/hit_test.dart:30`: hit testing returns the path of rects that
//!   contain the point; the leaf is first.
//!
//! `tap_hit_rect` and `hit` are pure functions of the laid-out rect, so these
//! tests pin them directly and add no ABI or IR field.

use opurom_render::gestures::MIN_HIT_SIZE;
use opurom_render::{
    gestures::{hit, tap_hit_rect},
    GestureEngine, PointerEvent, PointerKind, Rect,
};

fn rect(x: i64, y: i64, w: i64, h: i64) -> Rect {
    Rect { x, y, w, h }
}

#[test]
fn small_targets_expand_to_the_minimum_interactive_dimension() {
    // material/constants.dart:27.
    assert_eq!(MIN_HIT_SIZE, 48);
    let target = tap_hit_rect(rect(100, 100, 20, 20), false);
    assert_eq!(target, rect(86, 86, 48, 48));
}

#[test]
fn targets_larger_than_the_minimum_keep_their_rect() {
    let target = tap_hit_rect(rect(100, 100, 200, 60), false);
    assert_eq!(target, rect(100, 100, 200, 60));
}

#[test]
fn drag_only_surfaces_keep_their_visual_rect() {
    let target = tap_hit_rect(rect(100, 100, 20, 20), true);
    assert_eq!(target, rect(100, 100, 20, 20));
}

#[test]
fn degenerate_rects_stay_degenerate() {
    // A zero or negative size must not be expanded into an interactive one.
    assert_eq!(
        tap_hit_rect(rect(100, 100, 0, 0), false),
        rect(100, 100, 0, 0)
    );
    assert_eq!(
        tap_hit_rect(rect(100, 100, -10, -20), false),
        rect(100, 100, -10, -20)
    );
}

#[test]
fn hit_is_half_open_and_panic_free() {
    let bounds = rect(10, 10, 20, 20);
    assert!(hit(&bounds, 10, 10));
    assert!(hit(&bounds, 29, 29));
    assert!(!hit(&bounds, 30, 30));
    assert!(!hit(&bounds, 9, 10));
    // The upper edge saturates instead of overflowing.
    let extreme = rect(i64::MAX - 10, i64::MAX - 10, 40, 40);
    assert!(!hit(&extreme, i64::MAX, i64::MAX));
    assert!(!hit(&rect(0, 0, -5, 5), 0, 0));
}

#[test]
fn expanded_targets_change_what_the_engine_hits() {
    let mut e = GestureEngine::new();
    e.add_node_with(1, rect(100, 100, 20, 20), None, None);
    // The visual rect misses the point, but the expanded hit rect does not.
    assert_eq!(e.hit_test(90, 90), Some(1));
    assert_eq!(e.hit_test(60, 60), None);
    let _ = PointerEvent::at(PointerKind::Down, 90, 90, 0);
}

#[test]
fn target_geometry_is_deterministic() {
    let run = || {
        (
            tap_hit_rect(rect(100, 100, 20, 20), false),
            tap_hit_rect(rect(100, 100, 200, 60), false),
            tap_hit_rect(rect(100, 100, 20, 20), true),
            hit(&rect(10, 10, 20, 20), 30, 30),
        )
    };
    assert_eq!(run(), run());
}
