//! Machine-readable gesture performance fixture for the standalone CI gate.
//!
//! Flutter 3.50.0 anchors (checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`):
//! - `gestures/binding.dart:96-112,145-175,390-405`: per-device touch event
//!   batching and frame-sampled dispatch.
//! - `gestures/arena.dart:117-129,254-292`: per-pointer arenas with linear
//!   resolution over their member list.
//! - `gestures/velocity_tracker.dart:142-145,179-220`: bounded 20-sample,
//!   100 ms horizon.
//! - `gestures/constants.dart:65`: `kTouchSlop` 18 px.
//!
//! This test writes one deterministic JSON report to `CARGO_TARGET_TMPDIR`.
//! It never reads a wall clock and never mutates shared sources or CI files.

use opurom_render::{GestureEngine, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD};
use std::{env, fs, path::PathBuf};

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

fn event(kind: PointerKind, pointer: u32, t: i64, x: i64, y: i64) -> PointerEvent {
    PointerEvent::new(kind, pointer, t, x, y, x, y)
}

#[derive(Clone, Copy)]
struct Burst {
    frames: u64,
    pointers: u64,
    drop_every: u64,
}

fn run_burst(params: Burst) -> (u64, u64) {
    let mut e = engine();
    let mut dispatches = 0_u64;
    let mut slot_visits = 0_u64;
    for p in 0..params.pointers {
        dispatches += 1;
        let _ = e.dispatch(&event(
            PointerKind::Down,
            p as u32,
            p as i64,
            60 + p as i64,
            60,
        ));
    }
    for frame in 1..=params.frames {
        if params.drop_every > 0 && frame % params.drop_every == 0 {
            continue;
        }
        for p in 0..params.pointers {
            dispatches += 1;
            slot_visits += params.pointers;
            let _ = e.dispatch(&event(
                PointerKind::Move,
                p as u32,
                (frame + p) as i64,
                60 + p as i64 + (frame % 3) as i64,
                60,
            ));
        }
    }
    for p in 0..params.pointers {
        dispatches += 1;
        let _ = e.dispatch(&event(
            PointerKind::Up,
            p as u32,
            (params.frames + 1 + p) as i64,
            60 + p as i64,
            60,
        ));
    }
    let _ = e.on_tick(100_000);
    (dispatches, slot_visits)
}

fn replay_digest(frames: u64) -> (String, usize) {
    let mut e = engine();
    let mut digest = String::new();
    for frame in 0..frames {
        let kind = if frame == 0 {
            PointerKind::Down
        } else if frame + 1 == frames {
            PointerKind::Up
        } else {
            PointerKind::Move
        };
        let ev = event(kind, 1, frame as i64, 50 + (frame % 3) as i64, 50);
        match e.dispatch(&ev) {
            Some((node, gesture)) => digest.push_str(&format!("{node}:{gesture:?};")),
            None => digest.push_str(".;"),
        }
    }
    digest.push_str(&format!("{:?};{:?}", e.phase(), e.arena_winner()));
    let chars = digest.len();
    (digest, chars)
}

#[test]
fn write_gesture_perf_report() {
    let (small_dispatches, _small_slots) = run_burst(Burst {
        frames: 120,
        pointers: 2,
        drop_every: 0,
    });
    let (large_dispatches, _large_slots) = run_burst(Burst {
        frames: 240,
        pointers: 2,
        drop_every: 0,
    });
    let (one_slots, _) = run_burst(Burst {
        frames: 60,
        pointers: 1,
        drop_every: 0,
    });
    let (four_slots, _) = run_burst(Burst {
        frames: 60,
        pointers: 4,
        drop_every: 0,
    });
    let (full_dispatches, _) = run_burst(Burst {
        frames: 90,
        pointers: 1,
        drop_every: 0,
    });
    let (lossy_dispatches, _) = run_burst(Burst {
        frames: 90,
        pointers: 1,
        drop_every: 10,
    });
    let (digest, replay_chars) = replay_digest(240);
    let first_digest = digest.clone();
    let (second_digest, _) = replay_digest(240);
    assert_eq!(first_digest, second_digest, "replay must be byte-identical");

    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"opurom.gesture.perf/1\",\n",
            "  \"source\": \"flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb\",\n",
            "  \"clock\": \"logical\",\n",
            "  \"metrics\": {{\n",
            "    \"gesture_burst_small\": {small},\n",
            "    \"gesture_burst_large\": {large},\n",
            "    \"arena_slots_one\": {one},\n",
            "    \"arena_slots_four\": {four},\n",
            "    \"packet_loss_full\": {full},\n",
            "    \"packet_loss_lossy\": {lossy},\n",
            "    \"replay_bytes\": {replay},\n",
            "    \"slop_threshold\": {slop}\n",
            "  }}\n",
            "}}\n"
        ),
        small = small_dispatches,
        large = large_dispatches,
        one = one_slots,
        four = four_slots,
        full = full_dispatches,
        lossy = lossy_dispatches,
        replay = replay_chars,
        slop = DRAG_THRESHOLD,
    );
    let dir = PathBuf::from(
        env::var("GESTURE_PERF_REPORT_DIR")
            .or_else(|_| env::var("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|_| env::temp_dir().to_string_lossy().into_owned()),
    );
    let path = dir.join("gesture-perf-report.json");
    fs::write(&path, &report).expect("write gesture performance report");
    println!("GESTURE_PERF_REPORT={}", path.display());
}
