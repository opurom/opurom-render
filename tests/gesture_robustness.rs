//! Gesture and scroll robustness pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/velocity_tracker.dart:142-145,179-220`: tracker history is 20
//!   samples over a 100 ms horizon; no sample for more than 40 ms means the
//!   pointer is stopped; fewer than 3 samples produce no velocity.
//! - `physics/tolerance.dart:10-23`: default distance/time/velocity tolerance
//!   is 1e-3.
//! - `physics/spring_simulation.dart:257-280`: a simulation is done only when
//!   both position and velocity are within tolerance, and a scroll spring
//!   snaps to the exact end position at that point.
//! - `widgets/scroll_physics.dart:1021-1053`: an out-of-range release springs
//!   back; a fling already at an edge in its direction produces no activity.
//! - `gestures/arena.dart:157-179,232-292`: cancel removes a member and an
//!   arena resolves with exactly one callback per member.
//!
//! These cases pin extreme and malformed inputs to deterministic no-ops or
//! exact boundary values. No ABI or IR change is introduced.

use opurom_render::{
    scroll_physics::{
        IosFlingVelocityTracker, ScrollDriver, ScrollMotion, ScrollTolerance, MIN_FLING_VELOCITY,
    },
    GestureEngine, GestureKind, PointerEvent, PointerKind, Rect,
};

const START_MS: i64 = 50_000;

fn gesture_engine() -> GestureEngine {
    let mut e = GestureEngine::new();
    e.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
        None,
        None,
    );
    e
}

#[test]
fn malformed_pointer_coordinates_never_panic_or_emit() {
    let mut e = gesture_engine();
    for (x, y) in [
        (i64::MIN, i64::MIN),
        (i64::MAX, i64::MAX),
        (i64::MIN, i64::MAX),
        (i64::MAX, i64::MIN),
    ] {
        let mut engine = gesture_engine();
        assert_eq!(
            engine.dispatch(&PointerEvent::new(PointerKind::Down, 1, 0, x, y, x, y)),
            None
        );
        assert_eq!(
            engine.dispatch(&PointerEvent::new(PointerKind::Move, 1, 1, x, y, x, y)),
            None
        );
        assert_eq!(
            engine.dispatch(&PointerEvent::new(PointerKind::Up, 1, 2, x, y, x, y)),
            None
        );
    }
    assert_eq!(
        e.dispatch(&PointerEvent::new(PointerKind::Cancel, 9, 0, 0, 0, 0, 0)),
        None
    );
}

#[test]
fn cancel_with_zero_node_rejects_every_arena_member() {
    let mut e = gesture_engine();
    assert_eq!(
        e.dispatch(&PointerEvent::new(PointerKind::Down, 3, 0, 40, 40, 40, 40)),
        None
    );
    assert_eq!(
        e.dispatch(&PointerEvent::new(PointerKind::Cancel, 3, 1, 0, 0, 0, 0)),
        None
    );
    for kind in [
        GestureKind::Tap,
        GestureKind::DoubleTap,
        GestureKind::LongPress,
        GestureKind::Drag,
        GestureKind::Scale,
    ] {
        assert!(e.arena_rejected(kind));
    }
    // The cancel rejected every member, so no arena can have a winner.
    assert_eq!(e.arena_winner(), None);
    assert_eq!(e.on_tick(10_000), None);
}

#[test]
fn stopped_tracker_reports_zero_velocity_after_forty_ms() {
    let mut tracker = IosFlingVelocityTracker::new();
    for i in 0..6 {
        tracker.add_position(START_MS + i * 16, (i * 16) as f64, 0.0);
    }
    let live = tracker.estimate(START_MS + 80).unwrap();
    assert!(live.pixels_per_second.0 > 0.0);
    let stopped = tracker.estimate(START_MS + 121).unwrap();
    assert_eq!(stopped.pixels_per_second, (0.0, 0.0));
    assert_eq!(stopped.duration_ms, 0);
}

#[test]
fn malformed_fling_inputs_are_rejected() {
    for dpr in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(ScrollDriver::begin_fling(START_MS, 0.0, 1000.0, 0, 1000, dpr).is_none());
    }
    assert!(ScrollDriver::begin_fling(START_MS, f64::NAN, 1000.0, 0, 1000, 1.0).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, f64::INFINITY, 0, 1000, 1.0).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, 1000.0, 1000, 0, 1.0).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, 0.0, 0, 1000, 1.0).is_none());
    assert!(MIN_FLING_VELOCITY == 50.0);
}

#[test]
fn edge_fling_toward_the_edge_has_no_activity() {
    assert!(ScrollDriver::begin_fling(START_MS, 600.0, 1000.0, 0, 600, 1.0).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, -1000.0, 0, 600, 1.0).is_none());
    let spring = ScrollDriver::begin_fling(START_MS, 650.0, 1000.0, 0, 600, 1.0).unwrap();
    assert!(matches!(spring.motion, ScrollMotion::SpringBack(_)));
    assert!(spring.offset_px(START_MS + 5000) <= 600);
}

#[test]
fn tolerance_and_spring_end_are_exact() {
    let tolerance = ScrollTolerance::for_dpr(1.0);
    assert!((tolerance.distance - 1.0).abs() < f64::EPSILON);
    assert!((tolerance.velocity - 20.0).abs() < f64::EPSILON);
    let spring = ScrollDriver::begin_fling(START_MS, 610.0, 0.0, 0, 600, 1.0).unwrap();
    assert_eq!(spring.offset_px(START_MS + 10_000), 600);
}

#[test]
fn long_press_before_deadline_never_leaks_a_gesture() {
    let mut e = gesture_engine();
    assert_eq!(
        e.dispatch(&PointerEvent::new(PointerKind::Down, 1, 0, 60, 60, 60, 60)),
        None
    );
    assert_eq!(e.on_tick(499), None);
    assert_eq!(
        e.dispatch(&PointerEvent::new(PointerKind::Up, 1, 500, 60, 60, 60, 60)),
        None
    );
    // Releasing before the deadline rejects long press; the first tap then
    // wins by sweep after the double-tap window, never a long press.
    assert_eq!(e.arena_winner(), None);
    assert_eq!(e.on_tick(10_000), Some(opurom_render::Gesture::Tap));
    assert_ne!(e.arena_winner(), Some(GestureKind::LongPress));
}
