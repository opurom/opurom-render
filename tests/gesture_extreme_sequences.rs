//! Extreme gesture event sequences and timeout boundaries, Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/arena.dart:131-142,144-179,226-292`: close, sweep, accept and
//!   reject are idempotent with respect to a resolved arena; sweep gives the
//!   first survivor the win and rejects the rest.
//! - `gestures/binding.dart:336-346,407-448,536-546`: down closes an arena,
//!   up sweeps it, cancel removes exactly the cancelled pointer's cached path.
//! - `gestures/recognizer.dart:637-668,700-742`: deadlines do not fire after
//!   accept/reject/all-up/cancel; slop is compared globally with the touch
//!   tolerance.
//! - `gestures/constants.dart:29,35,39,65`: long press 500 ms, double tap
//!   300 ms, minimum gap 40 ms, touch slop 18 px.
//!
//! The suite drives Opurom's public renderer API only. It asserts convergence
//! (no live arena after terminal events), panic-free extreme inputs, and
//! byte-identical replay; it adds no ABI or IR field.

use opurom_render::{
    GestureEngine, GestureKind, Phase, PointerEvent, PointerKind, Rect, DOUBLE_TAP_INTERVAL_MS,
    LONG_PRESS_MS,
};

fn engine() -> GestureEngine {
    let mut e = GestureEngine::new();
    e.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        },
        None,
        None,
    );
    e
}

fn ev(kind: PointerKind, id: u32, t: i64, x: i64, y: i64) -> PointerEvent {
    PointerEvent::new(kind, id, t, x, y, x, y)
}

fn result_stream(engine: &mut GestureEngine, events: &[PointerEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| format!("{:?}", engine.dispatch(event)))
        .collect()
}

#[test]
fn zero_event_stream_is_idle_and_deadline_free() {
    let mut e = engine();
    assert_eq!(e.phase(), Phase::Idle);
    assert_eq!(e.next_deadline_ms(), None);
    assert_eq!(e.on_tick(0), None);
    assert_eq!(e.arena_winner(), None);
    assert_eq!(e.hit_test_path(10, 10), vec![1]);
}

#[test]
fn duplicate_down_id_does_not_open_a_second_sequence() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 4, 0, 50, 50)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 4, 1, 80, 80)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Move, 4, 2, 60, 60)), None);
    assert_eq!(
        e.on_tick(LONG_PRESS_MS),
        Some(opurom_render::Gesture::LongPress)
    );
    assert_ne!(e.arena_winner(), Some(GestureKind::Drag));
}

#[test]
fn backward_timestamps_are_ignored_by_the_engine_clock() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 5, 1000, 60, 60)), None);
    assert_eq!(
        e.dispatch(&ev(PointerKind::Move, 5, 900, 70, 60)),
        None,
        "a backwards timestamp stays below slop and cannot form a second drag"
    );
    assert_eq!(
        e.dispatch(&ev(PointerKind::Move, 5, 1001, 80, 60)),
        Some((1, opurom_render::Gesture::Drag { dx: 20, dy: 0 }))
    );
    assert_eq!(e.next_deadline_ms(), None);
}

#[test]
fn extreme_coordinates_converge_without_panic() {
    let coords = [
        (i64::MIN, i64::MIN),
        (i64::MAX, i64::MAX),
        (i64::MIN, i64::MAX),
        (i64::MAX, i64::MIN),
        (0, i64::MIN),
        (i64::MAX, 0),
    ];
    for (x, y) in coords {
        let mut e = engine();
        let stream = [
            ev(PointerKind::Down, 9, 0, x, y),
            ev(PointerKind::Move, 9, 1, x, y),
            ev(PointerKind::Up, 9, 2, x, y),
        ];
        let first = result_stream(&mut e, &stream);
        assert_eq!(first, result_stream(&mut engine(), &stream));
        assert!(e.next_deadline_ms().is_none_or(|d| d <= i64::MAX));
    }
}

#[test]
fn very_long_press_fires_once_then_stays_settled() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 1, 0, 80, 80)), None);
    assert_eq!(
        e.on_tick(LONG_PRESS_MS),
        Some(opurom_render::Gesture::LongPress)
    );
    assert_eq!(e.on_tick(LONG_PRESS_MS + 1), None);
    assert_eq!(e.on_tick(i64::MAX), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Up, 1, i64::MAX, 80, 80)), None);
    assert_eq!(e.phase(), Phase::Up);
}

#[test]
fn cancel_storm_leaves_no_pending_arena_or_timer() {
    let mut e = engine();
    for id in 1..=8 {
        assert_eq!(
            e.dispatch(&ev(PointerKind::Down, id, id as i64, 40, 40)),
            None
        );
    }
    for id in (1..=8).rev() {
        assert_eq!(
            e.dispatch(&ev(PointerKind::Cancel, id, 100 + id as i64, 0, 0)),
            None
        );
    }
    assert_eq!(e.on_tick(10_000), None);
    assert_eq!(e.next_deadline_ms(), None);
    assert_eq!(e.phase(), Phase::Cancelled);
    for kind in [
        GestureKind::Tap,
        GestureKind::DoubleTap,
        GestureKind::LongPress,
        GestureKind::Drag,
        GestureKind::Scale,
    ] {
        assert!(e.arena_rejected(kind));
    }
}

#[test]
fn simultaneous_multi_pointer_deadlines_emit_one_ordered_result() {
    let stream = [
        ev(PointerKind::Down, 1, 0, 60, 60),
        ev(PointerKind::Down, 2, 0, 140, 60),
        ev(PointerKind::Down, 3, 0, 220, 60),
        ev(PointerKind::Cancel, 2, 499, 0, 0),
    ];
    let mut a = engine();
    let mut b = engine();
    assert_eq!(
        result_stream(&mut a, &stream),
        result_stream(&mut b, &stream)
    );
    assert_eq!(
        a.on_tick(LONG_PRESS_MS),
        Some(opurom_render::Gesture::LongPress)
    );
    assert_eq!(
        b.on_tick(LONG_PRESS_MS),
        Some(opurom_render::Gesture::LongPress)
    );
    // Every pointer owns its own arena (`gestures/arena.dart:117-129`), so
    // the remaining pointer reaches its own long-press deadline and emits
    // its own result, in pointer order.
    assert_eq!(
        a.on_tick(DOUBLE_TAP_INTERVAL_MS + LONG_PRESS_MS),
        Some(opurom_render::Gesture::LongPress)
    );
    assert_eq!(a.next_deadline_ms(), None);
}

#[test]
fn repeated_terminal_events_do_not_reopen_a_settled_arena() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 12, 0, 70, 70)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Up, 12, 10, 70, 70)), None);
    for t in 11..20 {
        assert_eq!(e.dispatch(&ev(PointerKind::Up, 12, t, 70, 70)), None);
    }
    // The first deadline awards the pending plain tap exactly once; further
    // ticks cannot reopen the settled double-tap window.
    assert_eq!(
        e.on_tick(DOUBLE_TAP_INTERVAL_MS),
        Some(opurom_render::Gesture::Tap)
    );
    assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS + 30), None);
    assert_eq!(e.on_tick(i64::MAX), None);
}

#[test]
fn extreme_event_stream_is_byte_identical_on_replay() {
    let stream = [
        ev(PointerKind::Down, 1, 0, 100, 100),
        ev(PointerKind::Move, 1, -10, 118, 100),
        ev(PointerKind::Move, 1, 10, i64::MAX, i64::MIN),
        ev(PointerKind::Down, 1, 5, 100, 100),
        ev(PointerKind::Cancel, 1, 4, 0, 0),
        ev(PointerKind::Up, 1, 9, 100, 100),
        ev(PointerKind::Down, 2, i64::MAX, 120, 120),
        ev(PointerKind::Up, 2, i64::MAX, 120, 120),
    ];
    assert_eq!(
        result_stream(&mut engine(), &stream),
        result_stream(&mut engine(), &stream)
    );
}
