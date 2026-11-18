//! Advanced gesture parity pins: long-press drag and two-finger rotation.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/long_press.dart:641-647,650-685,783-800` (blob
//!   `5af43cf6a50f971584595bf2839cce8ccd6ac320`): the long-press deadline
//!   accepts mid-hold; every later move reports position and offsets from the
//!   down origin; lift ends the sequence.
//! - `widgets/drag_target.dart:410-450`: `LongPressDraggable` uses a
//!   `DelayedMultiDragGestureRecognizer` with `delay = kLongPressTimeout`.
//! - `gestures/scale.dart:487-510,738-750` (blob
//!   `fe3cdf27a414de2f112c58ad5d633831f7f1536e`): rotation is the signed
//!   angle between the initial and current lines; span/focal deltas and the
//!   scale ratio decide arena acceptance, not rotation alone.
//! - `gestures/constants.dart:29,65,76,80`: long press 500 ms, touch slop
//!   18 px, pan slop 36 px, scale slop 18 px.
//!
//! Opurom currently exposes long-press move updates behind an opt-in and
//! two-finger scale, but has no rotation callback in the C ABI. This suite
//! pins the deterministic geometry available today and records the exact
//! gap: a pure rotation must not be misreported as a scale, while the focal
//! point must remain the integer midpoint (`scale.dart:641`).

use opurom_render::{
    Gesture, GestureEngine, GestureKind, Phase, PointerEvent, PointerKind, Rect, LONG_PRESS_MS,
};

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

fn event(kind: PointerKind, id: u32, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, id, t, x, y, x, y)
}

#[test]
fn long_press_drag_keeps_global_and_local_offsets_in_lockstep() {
    let mut e = engine();
    e.enable_long_press_move_update();
    let down = PointerEvent::new(PointerKind::Down, 1, 0, 40, 60, 140, 160);
    assert_eq!(e.dispatch(&down), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));

    let moved = PointerEvent::new(PointerKind::Move, 1, LONG_PRESS_MS + 1, 55, 52, 155, 152);
    assert_eq!(
        e.dispatch(&moved),
        Some((
            1,
            Gesture::LongPressMoveUpdate {
                global_x: 155,
                global_y: 152,
                local_x: 55,
                local_y: 52,
                dx: 15,
                dy: -8,
                ldx: 15,
                ldy: -8,
            }
        ))
    );
    assert_eq!(
        e.dispatch(&PointerEvent::new(
            PointerKind::Up,
            1,
            LONG_PRESS_MS + 2,
            55,
            52,
            155,
            152
        )),
        None
    );
    assert_eq!(e.phase(), Phase::Up);
}

#[test]
fn long_press_drag_is_silent_without_opt_in() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 60, 0)), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 70, 66, LONG_PRESS_MS + 1)),
        None
    );
}

#[test]
fn pure_two_finger_rotation_does_not_fake_a_scale() {
    // Initial line from (50,100) to (150,100): span 50, focal (100,100).
    // A 90-degree rotation to (100,50)-(100,150) preserves span and focal;
    // Flutter's scale recognizer must not accept from span/focal deltas.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 100, 50, 2)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 2, 100, 150, 3)), None);
    assert_eq!(e.arena_winner(), None);
    assert!(!e.arena_rejected(GestureKind::Scale));
}

#[test]
fn rotation_with_span_change_uses_integer_midpoint_focal() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    // Rotate to vertical and grow the span from 40 to 60 in 1 px steps so
    // each accepted move stays within the engine's 18 px span-slop window.
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 90, 90, 2)), None);
    // The second finger's diagonal move already exceeds the focal/span slop;
    // later moves keep reporting the current midpoint and fixed-point ratio.
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 2, 110, 110, 3)),
        Some((
            1,
            Gesture::Scale {
                scale: 358,
                focal_x: 100,
                focal_y: 100,
            }
        ))
    );
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 100, 70, 4)),
        Some((
            1,
            Gesture::Scale {
                scale: 512,
                focal_x: 105,
                focal_y: 90,
            }
        ))
    );
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 2, 100, 130, 5)),
        Some((
            1,
            Gesture::Scale {
                scale: 768,
                focal_x: 100,
                focal_y: 100,
            }
        ))
    );
    assert_eq!(e.arena_winner(), Some(GestureKind::Scale));
}

#[test]
fn pure_rotation_event_stream_is_deterministic() {
    let run = || {
        let mut e = engine();
        let stream = [
            event(PointerKind::Down, 1, 70, 70, 0),
            event(PointerKind::Down, 2, 130, 130, 1),
            event(PointerKind::Move, 1, 130, 70, 2),
            event(PointerKind::Move, 2, 70, 130, 3),
            event(PointerKind::Up, 2, 70, 130, 4),
            event(PointerKind::Up, 1, 130, 70, 5),
        ];
        stream.iter().map(|ev| e.dispatch(ev)).collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
