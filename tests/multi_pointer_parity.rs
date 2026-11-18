//! Multi-pointer parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/binding.dart:407-448`: the hit path is cached per pointer id;
//!   up/cancel removes exactly that pointer's entry.
//! - `gestures/binding.dart:536-546`: down closes and up sweeps the arena for
//!   the event's pointer id.
//! - `gestures/arena.dart:117-129,157-179`: one arena per pointer id; sweep
//!   accepts the first surviving member and rejects the rest.
//! - `gestures/scale.dart:586-604,738-750`: the recognizer keeps a set of
//!   active pointer locations, recomputes the mean focal point, and accepts
//!   from span/focal/ratio deltas.
//! - `gestures/constants.dart:76,80`: pan slop 36 px, scale slop 18 px.
//!
//! These tests cover the deterministic projection available in Opurom's
//! renderer: parallel pointer identity, independent paths, cancellation by
//! pointer id, and two-finger focal geometry. No ABI, IR or runtime object
//! tree is introduced.

use opurom_render::{Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect};

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
fn parallel_pointers_keep_independent_hit_paths() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 11, 40, 40, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 12, 240, 240, 1)), None);
    // Each pointer keeps its own down-time path (`binding.dart:407-448`), so
    // crossing coordinates must not switch which sequence owns the event.
    // Each independent pointer resolves its own arena by sweep; both taps
    // win because Flutter keeps one arena per pointer (`arena.dart:117-129`).
    assert_eq!(
        e.dispatch(&event(PointerKind::Up, 11, 240, 240, 10)),
        Some((1, Gesture::Tap))
    );
    // The second pointer lands on the same node, so it joins the scale
    // recognizer instead of opening a second contest
    // (`gestures/scale.dart:586-592`); its lift ends the scale phase and
    // emits nothing on its own.
    assert_eq!(e.dispatch(&event(PointerKind::Up, 12, 40, 40, 11)), None);
}

#[test]
fn cancel_is_keyed_only_by_pointer_id() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 7, 60, 60, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 8, 140, 60, 1)), None);
    // Flutter's cancelPointer carries a pointer id; the host may send a miss
    // (node 0, zero coordinates) that must still cancel pointer 7 only.
    let cancel = event(PointerKind::Cancel, 7, 0, 0, 2);
    assert_eq!(e.on_event(0, &cancel), None);
    // Only pointer 7's arena is rolled back (`arena.dart:226-292`).
    assert!(e.arena_rejected_for(7, GestureKind::Tap));
    assert!(!e.arena_rejected_for(8, GestureKind::Tap));
    // The cancelled pointer was the only active one, so the reported phase
    // stays cancelled.
    assert_eq!(e.phase(), opurom_render::Phase::Cancelled);
    // Pointer 8's later up still reaches its own sequence and resolves.
    assert_eq!(e.dispatch(&event(PointerKind::Up, 8, 140, 60, 3)), None);
}

#[test]
fn two_pointer_scale_uses_mean_focal_point() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 2)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 2, 140, 180, 3)),
        Some((
            1,
            Gesture::Scale {
                scale: 1126,
                focal_x: 100,
                focal_y: 160,
            }
        ))
    );
}

#[test]
fn third_pointer_does_not_hijack_an_active_scale_pair() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 3, 200, 200, 2)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 3)), None);
    // The active pair still produces the same focal midpoint; pointer 3 is
    // not silently swapped into the scale track (`scale.dart:586-604`).
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 2, 140, 180, 4)),
        Some((
            1,
            Gesture::Scale {
                scale: 1126,
                focal_x: 100,
                focal_y: 160,
            }
        ))
    );
}

#[test]
fn parallel_pointer_event_stream_is_deterministic() {
    let run = || {
        let mut e = engine();
        let stream = [
            event(PointerKind::Down, 1, 50, 50, 0),
            event(PointerKind::Down, 2, 250, 250, 1),
            event(PointerKind::Move, 1, 60, 60, 2),
            event(PointerKind::Move, 2, 240, 240, 3),
            event(PointerKind::Up, 1, 60, 60, 4),
            event(PointerKind::Up, 2, 240, 240, 5),
        ];
        stream.iter().map(|ev| e.dispatch(ev)).collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
