//! Jank, timestamp jumps and refresh-rate tolerance pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/recognizer.dart:637-668,700-742`: deadlines are host timers
//!   armed at down and cancelled on accept/reject/up/cancel; acceptance compares
//!   global movement against slop, not frame count.
//! - `gestures/resampler.dart:15-35,130-171`: resampling is per pointer and
//!   sample time is supplied by the caller; only position/delta are resampled.
//! - `gestures/velocity_tracker.dart:142-145,179-220`: samples older than the
//!   100 ms horizon or with >40 ms gaps are excluded; no recent sample means
//!   zero velocity.
//! - `gestures/constants.dart:14,29,35,39,65`: press 100 ms, long press 500 ms,
//!   double-tap window 300 ms, minimum gap 40 ms, touch slop 18 px.
//!
//! The tests drive Opurom's public gesture API with host-injected timestamps
//! on 60/120/144 Hz grids and assert the same logical verdict. No ABI, IR or
//! shared source file is changed.

use opurom_render::{
    Gesture, GestureEngine, GestureKind, Phase, PointerEvent, PointerKind, Rect,
    DOUBLE_TAP_INTERVAL_MS, LONG_PRESS_MS,
};

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

fn ev(kind: PointerKind, id: u32, t: i64, x: i64, y: i64) -> PointerEvent {
    PointerEvent::new(kind, id, t, x, y, x, y)
}

fn grid(hz: i64, frames: i64) -> Vec<i64> {
    (0..=frames)
        .map(|i| 10_000 + (i * 1000 / hz).max(1))
        .collect()
}

fn verdict(events: &[PointerEvent], tick_ms: i64) -> (Option<String>, Phase, Option<GestureKind>) {
    let mut e = engine();
    let mut emitted = Vec::new();
    for event in events {
        if let Some((_, gesture)) = e.dispatch(event) {
            emitted.push(format!("{gesture:?}"));
        }
    }
    if let Some(gesture) = e.on_tick(tick_ms) {
        emitted.push(format!("{gesture:?}"));
    }
    (emitted.into_iter().last(), e.phase(), e.arena_winner())
}

#[test]
fn timestamp_jump_past_deadline_fires_long_press_once() {
    let events = [
        ev(PointerKind::Down, 1, 0, 60, 60),
        ev(PointerKind::Move, 1, 10_000, 61, 60),
    ];
    let (last, phase, winner) = verdict(&events, 10_001);
    assert_eq!(last, Some(format!("{:?}", Gesture::LongPress)));
    assert_eq!(phase, Phase::Triggered);
    assert_eq!(winner, Some(GestureKind::LongPress));
}

#[test]
fn jittery_moves_within_slop_keep_the_tap_alive() {
    let times = grid(144, 72);
    let mut events = vec![ev(PointerKind::Down, 2, times[0], 80, 80)];
    for (i, t) in times.iter().enumerate().take(60).skip(1) {
        let x = 80 + if i % 2 == 0 { 1 } else { -1 };
        events.push(ev(PointerKind::Move, 2, *t, x, 80));
    }
    events.push(ev(PointerKind::Up, 2, times[60] + 3, 80, 80));
    let (last, _, _) = verdict(&events, times[60] + 3 + DOUBLE_TAP_INTERVAL_MS);
    assert_eq!(last, Some(format!("{:?}", Gesture::Tap)));
}

#[test]
fn short_event_loss_recovers_from_the_last_committed_position() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 3, 0, 50, 50)), None);
    // A 60 Hz producer drops three frames; the next sample is 67 ms later but
    // still under touch slop, so no drag is fabricated from the gap.
    assert_eq!(e.dispatch(&ev(PointerKind::Move, 3, 67, 60, 60)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Up, 3, 84, 60, 60)), None);
    assert_eq!(e.on_tick(84 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
}

#[test]
fn duplicate_and_compensating_moves_do_not_win_drag() {
    let mut e = engine();
    assert_eq!(e.dispatch(&ev(PointerKind::Down, 4, 0, 100, 100)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Move, 4, 1, 118, 100)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Move, 4, 1, 118, 100)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Move, 4, 2, 100, 100)), None);
    assert_eq!(e.dispatch(&ev(PointerKind::Up, 4, 3, 100, 100)), None);
    assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS + 3), Some(Gesture::Tap));
}

#[test]
fn same_logical_long_press_on_60_120_144_hz_grids() {
    let mut results = Vec::new();
    for hz in [60, 120, 144] {
        let times = grid(hz, 40);
        let events = [
            ev(PointerKind::Down, 5, times[0], 70, 70),
            ev(PointerKind::Move, 5, times[10], 71, 70),
            ev(PointerKind::Move, 5, times[20], 70, 71),
        ];
        let verdict = verdict(&events, times[0] + LONG_PRESS_MS);
        results.push(verdict);
    }
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(results[0].0, Some(format!("{:?}", Gesture::LongPress)));
}

#[test]
fn same_logical_tap_on_60_120_144_hz_grids() {
    let mut results = Vec::new();
    for hz in [60, 120, 144] {
        let times = grid(hz, 30);
        let events = [
            ev(PointerKind::Down, 6, times[0], 120, 120),
            ev(PointerKind::Move, 6, times[3], 122, 120),
            ev(PointerKind::Up, 6, times[6], 120, 120),
        ];
        results.push(verdict(&events, times[0] + DOUBLE_TAP_INTERVAL_MS));
    }
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(results[0].0, Some(format!("{:?}", Gesture::Tap)));
    assert_eq!(results[0].1, Phase::Up);
}

#[test]
fn janky_stream_is_byte_identical_on_replay() {
    let stream = [
        ev(PointerKind::Down, 7, 100, 40, 40),
        ev(PointerKind::Move, 7, 133, 45, 40),
        ev(PointerKind::Move, 7, 134, 45, 40),
        ev(PointerKind::Move, 7, 250, 52, 41),
        ev(PointerKind::Cancel, 7, 251, 0, 0),
        ev(PointerKind::Move, 7, 260, 60, 60),
        ev(PointerKind::Up, 7, 261, 60, 60),
    ];
    assert_eq!(verdict(&stream, 10_000), verdict(&stream, 10_000));
}

#[test]
fn host_tick_before_or_at_deadline_is_ordered() {
    let events = [ev(PointerKind::Down, 8, 0, 90, 90)];
    assert_eq!(verdict(&events, LONG_PRESS_MS - 1).0, None);
    assert_eq!(
        verdict(&events, LONG_PRESS_MS).0,
        Some(format!("{:?}", Gesture::LongPress))
    );
}
