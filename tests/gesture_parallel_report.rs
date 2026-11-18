//! Machine-readable report for parallel pointer arenas.
//!
//! Flutter 3.50.0 anchors (checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`):
//! - `gestures/arena.dart:117-129`: one arena per pointer id.
//! - `gestures/arena.dart:226-292`: accept and reject run exactly once per
//!   member, and cancel rolls back its own arena.
//! - `gestures/multitap.dart:328-337,355`: a held tap is awarded when its
//!   double-tap window ends.
//! - `gestures/binding.dart:407-448`: a down-time hit path is cached per
//!   pointer id.
//!
//! The metrics are logical counts, never wall-clock timings. The test writes
//! one deterministic JSON report and never mutates shared sources or CI
//! files.

use opurom_render::{
    Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect, DOUBLE_TAP_INTERVAL_MS,
    LONG_PRESS_MS,
};
use std::{env, fs, path::PathBuf};

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
fn parallel_arenas_are_metrics_stable() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 3, 50, 50, 2)), None);
    // Pointer 3 lands on node 1, whose recognizers already have an active
    // pointer, so it joins the scale recognizer instead of opening a third
    // contest (`scale.dart:586-592`).
    assert_eq!(e.arena_winner_for(1), None);
    assert_eq!(e.arena_winner_for(2), None);
    let cancel = event(PointerKind::Cancel, 1, 0, 0, 3);
    assert_eq!(e.on_event(0, &cancel), None);
    assert!(e.arena_rejected_for(1, GestureKind::Tap));
    assert!(!e.arena_rejected_for(2, GestureKind::Tap));
    // Pointer 2 is still pending, so its long-press deadline remains visible.
    assert_eq!(
        e.next_deadline_ms(),
        Some(1 + LONG_PRESS_MS),
        "a cancelled pointer must not hide the other pointer's deadline"
    );
    assert_eq!(e.dispatch(&event(PointerKind::Up, 2, 150, 50, 4)), None);
    assert_eq!(e.on_tick(4 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    assert_eq!(e.arena_winner_for(2), Some(GestureKind::Tap));
}

#[test]
fn write_gesture_parallel_report() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)), None);
    // A drag on pointer 1 resolves its arena; pointer 2 keeps its own.
    assert_eq!(
        e.dispatch(&event(PointerKind::Move, 1, 80, 50, 2)),
        Some((1, Gesture::Drag { dx: 30, dy: 0 }))
    );
    let first_winner = e.arena_winner_for(1);
    let second_before = e.arena_winner_for(2);
    assert_eq!(e.dispatch(&event(PointerKind::Up, 2, 150, 50, 3)), None);
    assert_eq!(e.on_tick(3 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    let second_after = e.arena_winner_for(2);

    // Repeat the same sequence and confirm identical winners.
    let mut replay = engine();
    assert_eq!(
        replay.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)),
        None
    );
    assert_eq!(
        replay.dispatch(&event(PointerKind::Down, 2, 150, 50, 1)),
        None
    );
    assert_eq!(
        replay.dispatch(&event(PointerKind::Move, 1, 80, 50, 2)),
        Some((1, Gesture::Drag { dx: 30, dy: 0 }))
    );
    assert_eq!(replay.arena_winner_for(1), first_winner);
    assert_eq!(replay.arena_winner_for(2), second_before);
    // A fresh pointer keeps its long-press deadline visible.
    let mut deadline_engine = engine();
    assert_eq!(
        deadline_engine.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)),
        None
    );
    assert_eq!(deadline_engine.next_deadline_ms(), Some(LONG_PRESS_MS));

    let resolved = if first_winner.is_some() { 1 } else { 0 };
    let independent = if second_before.is_none() && second_after == Some(GestureKind::Tap) {
        1
    } else {
        0
    };
    let replay_stable = if replay.arena_winner_for(2) == second_before {
        1
    } else {
        0
    };
    let deadline = deadline_engine.next_deadline_ms().unwrap_or(0);
    assert!(deadline > 0, "a pending pointer still reports a deadline");

    assert!(resolved >= 1 && independent >= 1 && replay_stable >= 1);

    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"opurom.gesture.parallel/1\",\n",
            "  \"source\": \"flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb\",\n",
            "  \"clock\": \"logical\",\n",
            "  \"metrics\": {{\n",
            "    \"parallel_active_arenas\": 2,\n",
            "    \"parallel_resolved_arenas\": {resolved},\n",
            "    \"parallel_independent_wins\": {independent},\n",
            "    \"parallel_replay_stable\": {replay},\n",
            "    \"parallel_next_deadline_ms\": {deadline}\n",
            "  }}\n",
            "}}\n"
        ),
        resolved = resolved,
        independent = independent,
        replay = replay_stable,
        deadline = deadline,
    );
    let dir = PathBuf::from(
        env::var("GESTURE_PARALLEL_REPORT_DIR")
            .or_else(|_| env::var("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|_| env::temp_dir().to_string_lossy().into_owned()),
    );
    fs::create_dir_all(&dir).expect("create parallel report directory");
    let path = dir.join("gesture-parallel-report.json");
    fs::write(&path, &report).expect("write gesture parallel report");
    println!("GESTURE_PARALLEL_REPORT={}", path.display());
}
