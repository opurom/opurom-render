//! Gesture regression pins for Flutter 3.50.0 source rules.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/binding.dart:407-448`, blob
//!   `15f48b2c4ec0bff2da1a89acbc47b1ca6d16f25c`: move/up reuse the hit path
//!   captured at down, and up/cancel remove that cached path.
//! - `gestures/multitap.dart:266,943-946`, blob
//!   `b81d16aff4e41be7e283c0328a40301a1f135b9a`: a pending double-tap series
//!   is reset when the first touch travels more than `kDoubleTapTouchSlop`.
//! - `gestures/constants.dart:41-49`, blob
//!   `ae23eac1178aff50cbb5b0f29566a0e45589dd99`: `kDoubleTapTouchSlop` equals
//!   `kTouchSlop` (18 px); `kDoubleTapSlop` is 100 px.
//!
//! This file is additive: it never changes the renderer, ABI or IR.

use opurom_render::{Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect};

const TOUCH_SLOP: i64 = 18;

fn event(kind: PointerKind, pointer: u32, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, pointer, t, x, y, x, y)
}

fn engine_with_touch_node() -> GestureEngine {
    let mut engine = GestureEngine::new();
    engine.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 300,
        },
        None,
        None,
    );
    engine
}

#[test]
fn a_silent_up_releases_the_cached_hit_path() {
    let mut engine = engine_with_touch_node();
    // No explicit gesture prop: tap and double-tap recognizers both join, so
    // the first up waits for the 300 ms window and emits nothing.
    assert_eq!(engine.dispatch(&event(PointerKind::Down, 5, 20, 20, 0)), None);
    assert_eq!(engine.dispatch(&event(PointerKind::Up, 5, 20, 20, 10)), None);
    // A stray move from the released pointer must not reach node 1's pending
    // arena. Flutter removes the down-time hit test on up (`binding.dart:428-431`).
    assert_eq!(engine.dispatch(&event(PointerKind::Move, 5, 20, 20, 11)), None);
    assert_eq!(engine.arena_winner(), None);
    assert_eq!(engine.on_tick(300), Some(Gesture::Tap));
}

#[test]
fn double_tap_survives_first_touch_travel_below_touch_slop() {
    let mut engine = engine_with_touch_node();
    assert_eq!(engine.dispatch(&event(PointerKind::Down, 1, 40, 40, 0)), None);
    // 17 px is inside kDoubleTapTouchSlop; the series remains valid
    // (`multitap.dart:266`, `constants.dart:41-44`).
    assert_eq!(
        engine.dispatch(&event(PointerKind::Move, 1, 40 + TOUCH_SLOP - 1, 40, 5)),
        None
    );
    assert_eq!(engine.dispatch(&event(PointerKind::Up, 1, 57, 40, 10)), None);
    assert_eq!(engine.dispatch(&event(PointerKind::Down, 1, 57, 40, 60)), None);
    assert_eq!(
        engine.dispatch(&event(PointerKind::Up, 1, 57, 40, 70)),
        Some((1, Gesture::DoubleTap))
    );
}

#[test]
fn double_tap_resets_when_first_touch_exceeds_slop() {
    let mut engine = engine_with_touch_node();
    assert_eq!(engine.dispatch(&event(PointerKind::Down, 1, 40, 40, 0)), None);
    // 19 px is past kDoubleTapTouchSlop; Flutter resets the series
    // (`multitap.dart:943-946`). Past kTouchSlop the drag recognizer accepts
    // first (`monodrag.dart:688-710`), rejecting tap and double tap.
    assert_eq!(
        engine.dispatch(&event(PointerKind::Move, 1, 40 + TOUCH_SLOP + 1, 40, 5)),
        Some((1, Gesture::Drag { dx: 19, dy: 0 }))
    );
    assert!(engine.arena_rejected(GestureKind::Tap));
    assert!(engine.arena_rejected(GestureKind::DoubleTap));
}

#[test]
fn second_touch_still_uses_the_larger_double_tap_slop() {
    let mut engine = engine_with_touch_node();
    assert_eq!(engine.dispatch(&event(PointerKind::Down, 1, 40, 40, 0)), None);
    assert_eq!(engine.dispatch(&event(PointerKind::Up, 1, 40, 40, 10)), None);
    // 100 px is the larger second-touch tolerance (`constants.dart:46-49`).
    let second_x = 40 + 100;
    assert_eq!(
        engine.dispatch(&event(PointerKind::Down, 1, second_x, 40, 60)),
        None
    );
    assert_eq!(
        engine.dispatch(&event(PointerKind::Up, 1, second_x, 40, 70)),
        Some((1, Gesture::DoubleTap))
    );
}
