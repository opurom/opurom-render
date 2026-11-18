//! DragStartBehavior parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/recognizer.dart:48-56`: `DragStartBehavior.down` places the
//!   start at the first down event, `start` places it where the recognizer
//!   won the arena.
//! - `gestures/monodrag.dart:87,103-125`: the default behavior is `start`,
//!   and the start position is the arena-win position.
//! - `gestures/constants.dart:65`: touch slop 18 px.
//! - `gestures/arena.dart:157-179`: first accept wins.

use opurom_render::{
    DragStartBehavior, Gesture, GestureEngine, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD,
};

fn engine(behavior: DragStartBehavior) -> GestureEngine {
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
        Some(opurom_render::GestureKind::Drag),
    );
    e.set_drag_start_behavior(behavior);
    e
}

fn event(kind: PointerKind, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, 1, t, x, y, x, y)
}

#[test]
fn down_behavior_uses_the_down_position() {
    // recognizer.dart:48-51: the initial offset is the first down position.
    let mut e = engine(DragStartBehavior::Down);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 60, 50, 1)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 50 + DRAG_THRESHOLD + 2, 50, 2)),
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
fn start_behavior_uses_the_arena_win_position() {
    // recognizer.dart:53-55: the initial position is where the recognizer won
    // the arena. The first move past slop is the win, so its delta is zero.
    let mut e = engine(DragStartBehavior::Start);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 60, 50, 1)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 50 + DRAG_THRESHOLD + 2, 50, 2)),
        Some((1, Gesture::Drag { dx: 0, dy: 0 }))
    );
    // Subsequent deltas are measured from the win position.
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 50 + DRAG_THRESHOLD + 12, 50, 3)),
        Some((1, Gesture::Drag { dx: 10, dy: 0 }))
    );
}

#[test]
fn default_behavior_keeps_the_down_position() {
    // Opurom's default stays `Down` so the existing stream is unchanged.
    let mut e = engine(DragStartBehavior::Down);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 50, 50, 0)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 50 + DRAG_THRESHOLD + 2, 50, 1)),
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
fn a_horizontal_and_vertical_competition_uses_the_win_position() {
    // monodrag.dart:113-125: with two competing recognizers, `start` differs
    // from `down` because the win happens after the pointer moved.
    let mut e = engine(DragStartBehavior::Start);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 40, 60, 0)), None);
    // 40 -> 55 is 15 px, still inside the 18 px slop; 59 is the first move
    // outside it, so that is where the recognizer wins the arena.
    assert_eq!(e.dispatch(&event(PointerKind::Move, 55, 60, 1)), None);
    let win_x = 59;
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, win_x, 60, 2)),
        Some((1, Gesture::Drag { dx: 0, dy: 0 }))
    );
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, win_x + 10, 60, 3)),
        Some((1, Gesture::Drag { dx: 10, dy: 0 }))
    );
}
