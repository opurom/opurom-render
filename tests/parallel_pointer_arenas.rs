//! Parallel pointer arena pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb` (verified against the `3.50.0`
//! tag), engine `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/arena.dart:117-129`: the arena manager keeps one arena per
//!   pointer id, so two fingers run two independent contests.
//! - `gestures/arena.dart:131-142`: the down dispatch closes the arena of
//!   that pointer.
//! - `gestures/arena.dart:157-179,193-221`: sweep gives the first survivor the
//!   win and rejects the rest; a held arena is released later.
//! - `gestures/arena.dart:226-292`: accept and reject run exactly once per
//!   member, so a cancellation rolls back only its own arena.
//! - `gestures/scale.dart:586-592`: a second pointer on the same node joins
//!   the scale recognizer instead of starting a second sequence.
//! - `gestures/multitap.dart:328-337,355`: a tap is held for the double-tap
//!   window and awarded by the sweep when the window ends.
//!
//! These tests use the public gesture API and add no ABI or IR field.

use opurom_render::{
    Gesture, GestureEngine, GestureKind, Phase, PointerEvent, PointerKind, Rect,
    DOUBLE_TAP_INTERVAL_MS,
};

fn engine() -> GestureEngine {
    let mut e = GestureEngine::new();
    e.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        None,
        None,
    );
    e.add_node_with(
        2,
        Rect {
            x: 100,
            y: 0,
            w: 100,
            h: 100,
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
fn two_pointers_on_different_nodes_run_independent_contests() {
    // `gestures/arena.dart:117-129`: one arena per pointer id.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)), None);
    assert_eq!(e.arena_winner_for(1), None);
    assert_eq!(e.arena_winner_for(2), None);
    // Both taps are held for the double-tap window and awarded one per tick
    // (`gestures/multitap.dart:328-337,355`).
    assert_eq!(e.dispatch(&event(PointerKind::Up, 1, 50, 50, 10)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Up, 2, 150, 50, 11)), None);
    assert_eq!(e.on_tick(10 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    assert_eq!(e.on_tick(11 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    assert_eq!(e.arena_winner_for(1), Some(GestureKind::Tap));
    assert_eq!(e.arena_winner_for(2), Some(GestureKind::Tap));
}

#[test]
fn a_cancel_rolls_back_only_its_own_arena() {
    // `gestures/arena.dart:226-292`: the members of one arena are rejected
    // exactly once, and the other arena keeps its state.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)), None);
    let cancel = event(PointerKind::Cancel, 1, 0, 0, 2);
    assert_eq!(e.on_event(0, &cancel), None);
    assert!(e.arena_rejected_for(1, GestureKind::Tap));
    assert!(!e.arena_rejected_for(2, GestureKind::Tap));
    // The second pointer still resolves its own contest.
    assert_eq!(e.dispatch(&event(PointerKind::Up, 2, 150, 50, 3)), None);
    assert_eq!(e.on_tick(3 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    assert_eq!(e.arena_winner_for(2), Some(GestureKind::Tap));
}

#[test]
fn a_secondary_pointer_wins_while_the_primary_is_pending() {
    // The primary pointer waits for the long-press deadline; the secondary
    // pointer completes its own contest in the meantime.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Up, 2, 150, 50, 5)), None);
    assert_eq!(e.on_tick(5 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    // The primary sequence is untouched by the secondary pointer.
    assert_eq!(e.arena_winner_for(1), None);
    assert_eq!(e.phase(), Phase::Pending);
}

#[test]
fn the_primary_drag_survives_a_secondary_pointer() {
    // A pointer on another node must not disturb the primary sequence.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 80, 50, 2)),
        Some((1, Gesture::Drag { dx: 30, dy: 0 }))
    );
    assert_eq!(e.arena_winner_for(1), Some(GestureKind::Drag));
    assert_eq!(e.arena_winner_for(2), None);
}

#[test]
fn a_resolved_node_cannot_start_a_second_sequence() {
    // `gestures/scale.dart:586-592`: the recognizers of a node are shared, so
    // a second pointer on a resolved node joins the scale recognizer instead
    // of opening a second contest.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 80, 50, 1)),
        Some((1, Gesture::Drag { dx: 30, dy: 0 }))
    );
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 60, 50, 2)), None);
    // The second pointer does not produce a drag of its own.
    assert_eq!(e.dispatch(&event(PointerKind::Move, 2, 90, 50, 3)), None);
    assert_eq!(e.arena_winner_for(1), Some(GestureKind::Drag));
}

#[test]
fn parallel_recognition_is_deterministic() {
    let run = || {
        let mut e = engine();
        let mut stream: Vec<String> = Vec::new();
        stream.push(format!(
            "{:?}",
            e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0))
        ));
        stream.push(format!(
            "{:?}",
            e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1))
        ));
        stream.push(format!(
            "{:?}",
            e.dispatch(&event(PointerKind::Move, 1, 80, 50, 2))
        ));
        stream.push(format!(
            "{:?}",
            e.dispatch(&event(PointerKind::Move, 2, 170, 50, 3))
        ));
        stream.push(format!(
            "{:?}",
            e.dispatch(&event(PointerKind::Up, 1, 80, 50, 4))
        ));
        stream.push(format!(
            "{:?}",
            e.dispatch(&event(PointerKind::Up, 2, 170, 50, 5))
        ));
        stream.push(format!("{:?}", e.on_tick(5 + DOUBLE_TAP_INTERVAL_MS)));
        stream.push(format!("{:?}", e.arena_winner_for(1)));
        stream.push(format!("{:?}", e.arena_winner_for(2)));
        stream
    };
    assert_eq!(run(), run());
}
