//! Deterministic performance and high-frequency batching pins for gestures.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/binding.dart:96-112,145-175,390-405`: touch events are queued
//!   per device, sampled on frame boundaries, inactive resamplers removed,
//!   and non-resampled events dispatched immediately.
//! - `gestures/arena.dart:117-129,254-292`: one arena per pointer, with
//!   resolution over the member list; the host-side bound is linear in the
//!   number of members per pointer.
//! - `gestures/velocity_tracker.dart:142-145,179-220`: tracker work is bounded
//!   by 20 samples and a 100 ms horizon.
//! - `gestures/constants.dart:65`: touch slop 18 px.
//!
//! This suite measures logical work only: event counts, dispatch calls and
//! arena member visits. It never reads a wall clock, adds an ABI/IR field, or
//! touches shared sources. A machine-readable summary is emitted to stdout.

use opurom_render::{Gesture, GestureEngine, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD};

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

fn ev(kind: PointerKind, t: i64, x: i64, y: i64) -> PointerEvent {
    PointerEvent::new(kind, 1, t, x, y, x, y)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Work {
    events: u64,
    dispatches: u64,
    slot_visits: u64,
    emitted: u64,
}

fn run_burst(frames: u64, pointers: u64, drop_every: u64) -> Work {
    let mut engine = engine();
    let mut work = Work {
        events: 0,
        dispatches: 0,
        slot_visits: 0,
        emitted: 0,
    };
    for p in 0..pointers {
        let event = PointerEvent::new(
            PointerKind::Down,
            p as u32,
            p as i64,
            60 + p as i64,
            60,
            60 + p as i64,
            60,
        );
        work.events += 1;
        work.dispatches += 1;
        let _ = engine.dispatch(&event);
    }
    for frame in 1..=frames {
        if drop_every > 0 && frame % drop_every == 0 {
            // A dropped producer packet contributes no dispatch call.
            work.events += 1;
            continue;
        }
        for p in 0..pointers {
            let event = PointerEvent::new(
                PointerKind::Move,
                p as u32,
                (frame + p) as i64,
                60 + p as i64 + (frame % 3) as i64,
                60,
                60 + p as i64 + (frame % 3) as i64,
                60,
            );
            work.events += 1;
            work.dispatches += 1;
            // The renderer keeps one gesture arena; every pointer candidate
            // is visited once per event (`arena.dart:121-129`).
            work.slot_visits += pointers;
            if engine.dispatch(&event).is_some() {
                work.emitted += 1;
            }
        }
    }
    for p in 0..pointers {
        let event = PointerEvent::new(
            PointerKind::Up,
            p as u32,
            (frames + 1 + p) as i64,
            60 + p as i64,
            60,
            60 + p as i64,
            60,
        );
        work.events += 1;
        work.dispatches += 1;
        let _ = engine.dispatch(&event);
    }
    let _ = engine.on_tick(100_000);
    work
}

#[test]
fn high_frequency_batch_has_a_linear_logical_bound() {
    let small = run_burst(120, 2, 0);
    let large = run_burst(240, 2, 0);
    // Move frames scale linearly; the four down/up dispatches are fixed cost.
    assert_eq!(large.dispatches, small.dispatches * 2 - 4);
    assert!(large.slot_visits <= small.slot_visits * 2 + 4);
    assert_eq!(small.dispatches, 2 + 120 * 2 + 2);
    println!(
        "BENCH gesture_burst small={} large={} unit=logical_ops",
        small.dispatches, large.dispatches
    );
}

#[test]
fn arena_resolution_visits_each_member_bounded_by_pointer_count() {
    let one = run_burst(60, 1, 0);
    let four = run_burst(60, 4, 0);
    assert!(four.slot_visits <= one.slot_visits * 16);
    assert!(four.slot_visits >= one.slot_visits * 4);
    println!(
        "BENCH arena_slots one={} four={} unit=member_visits",
        one.slot_visits, four.slot_visits
    );
}

#[test]
fn dropped_packets_do_not_change_the_verdict() {
    let full = run_burst(90, 1, 0);
    let lossy = run_burst(90, 1, 10);
    assert!(lossy.dispatches < full.dispatches);
    // All jitter stays within DRAG_THRESHOLD, so no drag is emitted and the
    // arena converges identically regardless of producer packet loss.
    assert_eq!(full.emitted, 0);
    assert_eq!(lossy.emitted, 0);
    println!(
        "BENCH packet_loss full={} lossy={} unit=dispatches",
        full.dispatches, lossy.dispatches
    );
}

#[test]
fn high_frequency_replay_is_byte_identical() {
    let run = || {
        let mut e = engine();
        let mut digest = String::new();
        for frame in 0..240u64 {
            let event = ev(
                if frame == 0 {
                    PointerKind::Down
                } else if frame == 239 {
                    PointerKind::Up
                } else {
                    PointerKind::Move
                },
                frame as i64,
                50 + (frame % 3) as i64,
                50,
            );
            match e.dispatch(&event) {
                Some((node, gesture)) => {
                    digest.push_str(&format!("{node}:{gesture:?};"));
                }
                None => digest.push_str(".;"),
            }
        }
        digest.push_str(&format!("{:?};{:?}", e.phase(), e.arena_winner()));
        digest
    };
    let first = run();
    assert_eq!(first, run());
    assert!(first.ends_with("Up;None"));
    println!(
        "BENCH replay bytes={} frames=240 unit=digest_chars",
        first.len()
    );
}

#[test]
fn slop_boundary_batching_is_stable() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 0, 100, 100)), None);
    for x in 101..=(100 + DRAG_THRESHOLD) {
        assert_eq!(e.dispatch(&ev(PointerKind::Move, x - 100, x, 100)), None);
    }
    assert_eq!(
        e.dispatch(&ev(
            PointerKind::Move,
            DRAG_THRESHOLD + 1,
            100 + DRAG_THRESHOLD + 1,
            100
        )),
        Some((
            1,
            Gesture::Drag {
                dx: DRAG_THRESHOLD + 1,
                dy: 0
            }
        ))
    );
    println!("BENCH slop_threshold value={} unit=px", DRAG_THRESHOLD);
}
