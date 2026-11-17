//! Long-press move updates against Flutter `LongPressGestureRecognizer`.
//!
//! Flutter anchor: flutter 3.50 commit ab2912035e160d9cad6306df307576da6128ebdb,
//! `packages/flutter/lib/src/gestures/long_press.dart` blob
//! 5af43cf6a50f971584595bf2839cce8ccd6ac320: `:176-196`
//! `LongPressMoveUpdateDetails`, `:641-644` deadline acceptance,
//! `:650-685` accepted-pointer dispatch, `:783-800` move callback.
//! Slop from `constants.dart` blob
//! ae23eac1178aff50cbb5b0f29566a0e45589dd99: `:65` kTouchSlop 18.

use opurom_render::{
    Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD,
    LONG_PRESS_MS,
};

fn engine(enable: bool) -> GestureEngine {
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
    if enable {
        e.enable_long_press_move_update();
    }
    e
}

fn at(kind: PointerKind, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, 0, t, x, y, x, y)
}

#[test]
fn accepted_long_press_reports_every_move_with_both_offsets() {
    // long_press.dart:641,783: acceptance happens at the deadline, then every
    // move reports position and offset from the down origin.
    let mut e = engine(true);
    assert_eq!(e.dispatch(&at(PointerKind::Down, 60, 70, 0)), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
    assert_eq!(
        e.dispatch(&at(PointerKind::Move, 70, 66, LONG_PRESS_MS + 1)),
        Some((
            1,
            Gesture::LongPressMoveUpdate {
                global_x: 70,
                global_y: 66,
                local_x: 70,
                local_y: 66,
                dx: 10,
                dy: -4,
                ldx: 10,
                ldy: -4,
            }
        ))
    );
    assert_eq!(
        e.dispatch(&at(PointerKind::Move, 100, 90, LONG_PRESS_MS + 2)),
        Some((
            1,
            Gesture::LongPressMoveUpdate {
                global_x: 100,
                global_y: 90,
                local_x: 100,
                local_y: 90,
                dx: 40,
                dy: 20,
                ldx: 40,
                ldy: 20,
            }
        ))
    );
    assert_eq!(e.arena_winner(), Some(GestureKind::LongPress));
}

#[test]
fn local_and_global_frames_stay_separate() {
    // long_press.dart:786-793: localOffsetFromOrigin uses localPosition, not
    // the global delta. A scrolled host supplies both frames independently.
    let mut e = engine(true);
    let down = PointerEvent::new(PointerKind::Down, 0, 0, 20, 30, 100, 130);
    assert_eq!(e.dispatch(&down), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
    let mv = PointerEvent::new(PointerKind::Move, 0, LONG_PRESS_MS + 1, 30, 24, 115, 122);
    assert_eq!(
        e.dispatch(&mv),
        Some((
            1,
            Gesture::LongPressMoveUpdate {
                global_x: 115,
                global_y: 122,
                local_x: 30,
                local_y: 24,
                dx: 15,
                dy: -8,
                ldx: 10,
                ldy: -6,
            }
        ))
    );
}

#[test]
fn default_engine_does_not_emit_move_updates() {
    // Opt-in only: the legacy stream stays byte-identical for existing users.
    let mut e = engine(false);
    assert_eq!(e.dispatch(&at(PointerKind::Down, 60, 70, 0)), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
    assert_eq!(
        e.dispatch(&at(PointerKind::Move, 70, 66, LONG_PRESS_MS + 1)),
        None
    );
}

#[test]
fn move_before_the_deadline_still_rejects_long_press() {
    // long_press.dart:679: a move past touch slop before acceptance cancels.
    let mut e = engine(true);
    assert_eq!(e.dispatch(&at(PointerKind::Down, 60, 70, 0)), None);
    assert_eq!(
        e.dispatch(&at(PointerKind::Move, 60 + DRAG_THRESHOLD + 1, 70, 10)),
        Some((1, Gesture::Drag { dx: 19, dy: 0 }))
    );
    assert_eq!(e.on_tick(LONG_PRESS_MS), None);
    assert_eq!(e.arena_winner(), Some(GestureKind::Drag));
}

#[test]
fn lift_and_later_moves_are_silent_after_a_long_press() {
    let mut e = engine(true);
    assert_eq!(e.dispatch(&at(PointerKind::Down, 60, 70, 0)), None);
    assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
    assert_eq!(
        e.dispatch(&at(PointerKind::Up, 70, 70, LONG_PRESS_MS + 1)),
        None
    );
    assert_eq!(
        e.dispatch(&at(PointerKind::Move, 80, 70, LONG_PRESS_MS + 2)),
        None
    );
}
