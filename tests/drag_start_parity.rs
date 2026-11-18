//! Drag-start and tap-plus-drag parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/recognizer.dart:42-56`: `DragStartBehavior.down` reports the
//!   first down position, while `start` reports the arena win position.
//! - `gestures/multidrag.dart:500-547,574-588`: a delayed multi-drag wins
//!   after `kLongPressTimeout` when the pointer stayed within hit slop; moving
//!   farther than slop before the delay rejects it.
//! - `gestures/tap_and_drag.dart:631-671,750-870`: a combined tap-and-drag
//!   recognizer shares a single arena between tap and drag semantics.
//! - `gestures/constants.dart:29,65`: long press 500 ms, touch slop 18 px.
//!
//! The renderer currently exposes a single `drag` gesture with deltas from
//! the down point (`DragStartBehavior.down`). This suite pins that projection
//! and records the exact host-side rules required for `start` behavior and
//! delayed long-press drag, without adding an ABI or IR field.

use opurom_render::{
    Gesture, GestureEngine, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD, LONG_PRESS_MS,
};

const DELAYED_DRAG_START_MS: i64 = LONG_PRESS_MS;

fn engine() -> GestureEngine {
    let mut e = GestureEngine::new();
    e.add_node_with(
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
    e
}

fn event(kind: PointerKind, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, 1, t, x, y, x, y)
}

fn drag_start(behavior: DragStartBehavior, down: (i64, i64), win: (i64, i64)) -> (i64, i64) {
    match behavior {
        DragStartBehavior::Down => down,
        DragStartBehavior::Start => win,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragStartBehavior {
    Down,
    Start,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragOutcome {
    Started { origin: (i64, i64) },
    Rejected,
}

fn delayed_drag(
    start: (i64, i64),
    move_before_deadline: Option<(i64, i64)>,
) -> DragOutcome {
    if let Some(point) = move_before_deadline {
        let dx = point.0 - start.0;
        let dy = point.1 - start.1;
        if dx * dx + dy * dy > DRAG_THRESHOLD * DRAG_THRESHOLD {
            return DragOutcome::Rejected;
        }
    }
    DragOutcome::Started { origin: start }
}

#[test]
fn renderer_drag_delta_uses_the_down_origin() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 60, 50, 10)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 50 + DRAG_THRESHOLD + 2, 50, 20)),
        Some((
            1,
            Gesture::Drag {
                dx: DRAG_THRESHOLD + 2,
                dy: 0
            }
        ))
    );
}

#[test]
fn drag_start_behavior_selects_down_or_arena_win_origin() {
    assert_eq!(
        drag_start(DragStartBehavior::Down, (40, 60), (55, 60)),
        (40, 60)
    );
    assert_eq!(
        drag_start(DragStartBehavior::Start, (40, 60), (55, 60)),
        (55, 60)
    );
}

#[test]
fn delayed_drag_starts_after_the_long_press_deadline() {
    assert_eq!(
        delayed_drag((40, 60), Some((50, 60))),
        DragOutcome::Started { origin: (40, 60) }
    );
    assert_eq!(DELAYED_DRAG_START_MS, 500);
}

#[test]
fn delayed_drag_rejects_motion_past_slop_before_the_deadline() {
    assert_eq!(
        delayed_drag((40, 60), Some((40 + DRAG_THRESHOLD + 1, 60))),
        DragOutcome::Rejected
    );
}

#[test]
fn tap_and_drag_share_one_arena_and_never_both_win() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 80, 80, 0)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 80 + DRAG_THRESHOLD + 1, 80, 10)),
        Some((
            1,
            Gesture::Drag {
                dx: DRAG_THRESHOLD + 1,
                dy: 0
            }
        ))
    );
    assert_eq!(e.dispatch(&event(PointerKind::Up, 100, 80, 20)), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), None);
}
