//! Two-finger scale recognition against Flutter `scale.dart`.
//!
//! Flutter anchor: flutter 3.50 commit ab2912035e160d9cad6306df307576da6128ebdb,
//! `packages/flutter/lib/src/gestures/scale.dart` blob
//! fe3cdf27a414de2f112c58ad5d633831f7f1536e: `:345` recognizer, `:455` span
//! ratio, `:641` focal mean, `:744-748` slop acceptance, `:806` acceptGesture.
//! Slop values from `constants.dart` blob
//! ae23eac1178aff50cbb5b0f29566a0e45589dd99: `:65` kTouchSlop 18,
//! `:76` kPanSlop 36, `:80` kScaleSlop == kTouchSlop.

use opurom_render::{
    Gesture, GestureEngine, GestureKind, Phase, PointerEvent, PointerKind, Rect, SCALE_SLOP,
    SCALE_UNIT,
};

fn engine() -> GestureEngine {
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

fn event(kind: PointerKind, id: u32, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, id, t, x, y, x, y)
}

fn down(e: &mut GestureEngine, id: u32, x: i64, y: i64, t: i64) {
    assert_eq!(e.dispatch(&event(PointerKind::Down, id, x, y, t)), None);
}

#[test]
fn two_pointers_accept_scale_after_span_slop() {
    // scale.dart:436-437,641: span is the mean pointer deviation from the
    // focal point, i.e. half the separation for two pointers. Initial span 40;
    // moving one finger 40 px gives span 60, a 20 px change above kScaleSlop 18.
    // scale.dart:455: fixed-point scale = currentSpan / initialSpan.
    let mut e = engine();
    down(&mut e, 1, 60, 40, 0);
    down(&mut e, 2, 140, 40, 10);
    assert_eq!(e.arena_winner(), None);
    // Below slop: span 45, delta 5.
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 70, 40, 20)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 2, 160, 40, 21)), None);
    assert_eq!(e.arena_winner(), None);
    // Above slop on the first finger: span 60, delta 20, focal 100.
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 40, 40, 30)),
        Some((
            1,
            Gesture::Scale {
                scale: 1536,
                focal_x: 100,
                focal_y: 40,
            }
        ))
    );
    assert_eq!(e.arena_winner(), Some(GestureKind::Scale));
    assert!(e.arena_rejected(GestureKind::Drag));
    assert_eq!(SCALE_SLOP, 18);
    assert_eq!(SCALE_UNIT, 1024);
}

#[test]
fn scale_updates_track_either_pointer_and_keeps_focal_at_the_midpoint() {
    // scale.dart:597,641: every accepted move emits the current focal point,
    // the mean of the two pointer positions.
    let mut e = engine();
    down(&mut e, 1, 60, 60, 0);
    down(&mut e, 2, 140, 60, 1);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 50, 60, 2)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 2, 170, 60, 3)),
        Some((
            1,
            Gesture::Scale {
                scale: 1536,
                focal_x: 110,
                focal_y: 60,
            }
        ))
    );
    // Move the first pointer again: the gesture keeps flowing from either id.
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 10, 70, 4)),
        Some((
            1,
            Gesture::Scale {
                scale: 2048,
                focal_x: 90,
                focal_y: 65,
            }
        ))
    );
}

#[test]
fn focal_move_alone_accepts_at_pan_slop() {
    // scale.dart:747: focalPointDelta > computePanSlop (constants.dart:76, 36).
    let mut e = engine();
    down(&mut e, 1, 60, 100, 0);
    down(&mut e, 2, 140, 100, 1);
    // First finger +36: span change 18 (not > 18), focal delta 18.
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 96, 100, 2)), None);
    // Second finger +38: span 41, focal delta 37 > 36.
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 2, 178, 100, 3)),
        Some((
            1,
            Gesture::Scale {
                scale: 1049,
                focal_x: 137,
                focal_y: 100,
            }
        ))
    );
}

#[test]
fn a_single_pointer_never_scales() {
    // scale.dart:427: one pointer cannot form a span; the drag recognizer wins.
    let mut e = engine();
    down(&mut e, 1, 80, 80, 0);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 60, 80, 1)),
        Some((1, Gesture::Drag { dx: -20, dy: 0 }))
    );
    assert_eq!(e.arena_winner(), Some(GestureKind::Drag));
    assert!(!e.arena_rejected(GestureKind::Drag));
}

#[test]
fn drag_wins_the_arena_when_it_accepts_before_the_second_finger() {
    // arena.dart:157 first accept wins; scale cannot reopen a resolved arena.
    let mut e = engine();
    down(&mut e, 1, 80, 80, 0);
    let _ = e.dispatch(&event(PointerKind::Move, 1, 50, 80, 1));
    down(&mut e, 2, 140, 80, 2);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 2, 180, 80, 3)), None);
    assert_eq!(e.arena_winner(), Some(GestureKind::Drag));
}

#[test]
fn lifting_either_pointer_ends_the_scale_phase() {
    // scale.dart:600-604: a pointer up reconfigures the pointer locations.
    let mut e = engine();
    down(&mut e, 1, 60, 40, 0);
    down(&mut e, 2, 140, 40, 1);
    let _ = e.dispatch(&event(PointerKind::Move, 1, 20, 40, 2));
    assert_eq!(e.dispatch(&event(PointerKind::Up, 2, 160, 40, 3)), None);
    assert_eq!(e.arena_winner(), Some(GestureKind::Scale));
    // The remaining pointer still runs its own single-pointer sequence, but a
    // scale winner emits no drag.
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 10, 40, 4)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Up, 1, 10, 40, 5)), None);
}

#[test]
fn scale_kind_parses_and_rejects_unknown_values() {
    assert_eq!(GestureKind::parse("scale"), Some(GestureKind::Scale));
    assert_eq!(GestureKind::Scale.name(), "scale");
    assert_eq!(GestureKind::parse("pinch"), None);
}

#[test]
fn cancel_is_keyed_only_by_pointer_id_and_time() {
    // Flutter cancelPointer carries only a pointer id (binding.dart:336-346,
    // blob 15f48b2c4ec0bff2da1a89acbc47b1ca6d16f25c). The C bridge may send
    // node_id 0 and zero coordinates; on_cancel locates the down by id.
    let mut e = engine();
    down(&mut e, 7, 60, 40, 0);
    let cancel = event(PointerKind::Cancel, 7, 0, 0, 1);
    assert_eq!(e.on_event(0, &cancel), None);
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
    assert_eq!(e.dispatch(&cancel), None);
}
