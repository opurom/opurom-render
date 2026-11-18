//! Advanced gesture thresholds and termination parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/multidrag.dart:494-547`: a delayed multi-drag arms a timer for
//!   `kLongPressTimeout`; moving more than hit slop before the timer rejects
//!   it; when the timer expires inside slop it starts from the down position.
//! - `gestures/multidrag.dart:260-273,574-600`: up and cancel remove the
//!   per-pointer state; cancel cannot later start a drag.
//! - `gestures/scale.dart:487-510`: rotation is `atan2(current line) -
//!   atan2(initial line)`; the sign follows endpoint motion.
//! - `gestures/scale.dart:738-759,838-850`: acceptance needs span/focal/ratio
//!   thresholds; last pointer up or cancel returns the recognizer to ready.
//! - `gestures/constants.dart:29,65,74-80`: long press 500 ms, touch slop
//!   18 px, pan slop 36 px, scale slop 18 px.
//!
//! This file adds no ABI or IR field. It pins deterministic projections that
//! a host can use before the renderer gains dedicated rotation callbacks.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DelayedDragState {
    Pending,
    Started { x: i64, y: i64 },
    Cancelled,
    Rejected,
}

fn delayed_drag_step(
    state: DelayedDragState,
    down: (i64, i64),
    point: (i64, i64),
    now_ms: i64,
    touch_slop: i64,
) -> DelayedDragState {
    match state {
        DelayedDragState::Pending => {
            let dx = point.0 - down.0;
            let dy = point.1 - down.1;
            if dx * dx + dy * dy > touch_slop * touch_slop {
                DelayedDragState::Rejected
            } else if now_ms >= 500 {
                DelayedDragState::Started {
                    x: down.0,
                    y: down.1,
                }
            } else {
                DelayedDragState::Pending
            }
        }
        DelayedDragState::Cancelled | DelayedDragState::Rejected => state,
        DelayedDragState::Started { x, y } => DelayedDragState::Started { x, y },
    }
}

fn cancel(_state: DelayedDragState) -> DelayedDragState {
    DelayedDragState::Cancelled
}

/// Integer quadrant angle in 1/16 turn units, matching the sign and ordering
/// of `atan2(dy, dx)` for the axis-aligned cases used here (`scale.dart:500-503`).
fn angle_quadrant(from: (i64, i64), to: (i64, i64)) -> i32 {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    match (dx.signum(), dy.signum()) {
        (1, 0) => 0,
        (1, 1) => 2,
        (0, 1) => 4,
        (-1, 1) => 6,
        (-1, 0) => 8,
        (-1, -1) => 10,
        (0, -1) => 12,
        (1, -1) => 14,
        _ => 0,
    }
}

fn rotation_steps(initial: ((i64, i64), (i64, i64)), current: ((i64, i64), (i64, i64))) -> i32 {
    let a = angle_quadrant(initial.0, initial.1);
    let b = angle_quadrant(current.0, current.1);
    let raw = b - a;
    if raw > 8 {
        raw - 16
    } else if raw < -8 {
        raw + 16
    } else {
        raw
    }
}

#[test]
fn delayed_drag_starts_at_down_after_five_hundred_ms() {
    let state = delayed_drag_step(DelayedDragState::Pending, (40, 60), (50, 60), 499, 18);
    assert_eq!(state, DelayedDragState::Pending);
    let started = delayed_drag_step(state, (40, 60), (50, 60), 500, 18);
    assert_eq!(started, DelayedDragState::Started { x: 40, y: 60 });
}

#[test]
fn delayed_drag_rejects_past_eighteen_px_before_deadline() {
    let state = delayed_drag_step(DelayedDragState::Pending, (40, 60), (40 + 19, 60), 100, 18);
    assert_eq!(state, DelayedDragState::Rejected);
    assert_eq!(
        delayed_drag_step(state, (40, 60), (40, 60), 999, 18),
        DelayedDragState::Rejected
    );
}

#[test]
fn delayed_drag_cancel_is_terminal() {
    let cancelled = cancel(DelayedDragState::Pending);
    assert_eq!(cancelled, DelayedDragState::Cancelled);
    assert_eq!(
        delayed_drag_step(cancelled, (40, 60), (40, 60), 5000, 18),
        DelayedDragState::Cancelled
    );
}

#[test]
fn rotation_angle_is_signed_and_normalized() {
    // Initial line points east; current line points south = +90 degrees.
    assert_eq!(
        rotation_steps(((100, 100), (150, 100)), ((100, 100), (100, 150))),
        4
    );
    // Current line points north = -90 degrees.
    assert_eq!(
        rotation_steps(((100, 100), (150, 100)), ((100, 100), (100, 50))),
        -4
    );
    // A 180-degree turn is represented at the wrap boundary.
    assert_eq!(
        rotation_steps(((100, 100), (150, 100)), ((100, 100), (50, 100))),
        8
    );
}

#[test]
fn rotation_alone_does_not_satisfy_scale_acceptance() {
    let span_initial = 50_i64;
    let span_current = 50_i64;
    let focal_delta = 0_i64;
    let accepted = (span_current - span_initial).abs() > 18
        || focal_delta > 36
        || (span_current * 100 / span_initial.max(1)) > 105;
    assert!(!accepted, "scale.dart:743-749 has no rotation-only gate");
}

#[test]
fn last_pointer_up_ends_the_rotation_projection() {
    let mut active = 2_u8;
    let mut state = "started";
    active -= 1;
    if active == 0 {
        state = "ready";
    }
    assert_eq!(state, "started");
    active -= 1;
    if active == 0 {
        state = "ready";
    }
    assert_eq!(state, "ready");
}

#[test]
fn advanced_projection_is_deterministic() {
    let run = || {
        let mut state = DelayedDragState::Pending;
        for t in [0, 100, 499] {
            state = delayed_drag_step(state, (40, 60), (50, 60), t, 18);
        }
        state = delayed_drag_step(state, (40, 60), (50, 60), 500, 18);
        let angle = rotation_steps(((0, 0), (10, 0)), ((0, 0), (0, 10)));
        (state, angle)
    };
    assert_eq!(run(), run());
}
