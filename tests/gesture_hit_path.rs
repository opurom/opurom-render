//! Pointer path reuse against Flutter 3.50.0 `gestures/binding.dart`.
//!
//! Source pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`, file blob
//! `15f48b2c4ec0bff2da1a89acbc47b1ca6d16f25c`. Lines 407-448 reuse the
//! `HitTestResult` captured at down for move/up/cancel; a fresh hit test would
//! let a pointer that left the node silently escape its own gesture arena.

use opurom_render::{Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect};

fn event(kind: PointerKind, pointer: u32, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, pointer, t, x, y, x, y)
}

#[test]
fn move_and_up_reuse_the_down_hit_path() {
    let mut engine = GestureEngine::new();
    engine.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 40,
        },
        None,
        Some(GestureKind::Tap),
    );
    engine.add_node_with(
        2,
        Rect {
            x: 100,
            y: 100,
            w: 40,
            h: 40,
        },
        None,
        Some(GestureKind::Tap),
    );

    assert_eq!(
        engine.dispatch(&event(PointerKind::Down, 7, 20, 20, 0)),
        None
    );
    // Moving outside node 1 must still dispatch to node 1's sequence; a
    // re-hit-test would find nothing or pick node 2 and lose the tap.
    assert_eq!(
        engine.dispatch(&event(PointerKind::Move, 7, 120, 120, 10)),
        None
    );
    assert_eq!(
        engine.dispatch(&event(PointerKind::Up, 7, 120, 120, 10)),
        Some((1, Gesture::Tap)),
        "the down-time path must survive a move outside the original bounds"
    );
}

#[test]
fn pointer_paths_are_independent_and_released_on_up() {
    let mut engine = GestureEngine::new();
    engine.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 40,
        },
        None,
        Some(GestureKind::Tap),
    );
    engine.add_node_with(
        2,
        Rect {
            x: 100,
            y: 100,
            w: 40,
            h: 40,
        },
        None,
        Some(GestureKind::Tap),
    );

    assert_eq!(
        engine.dispatch(&event(PointerKind::Down, 1, 20, 20, 0)),
        None
    );
    assert_eq!(
        engine.dispatch(&event(PointerKind::Down, 2, 120, 120, 1)),
        None
    );
    // Each pointer stays on the node it pressed even when the coordinates
    // cross; the arena is per pointer (`arena.dart:117-129`).
    assert_eq!(
        engine.dispatch(&event(PointerKind::Up, 1, 120, 120, 10)),
        Some((1, Gesture::Tap)),
        "pointer 1 must resolve on node 1, not on the node under its lift point"
    );
}
