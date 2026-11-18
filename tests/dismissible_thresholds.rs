//! Dismissible gesture threshold parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `b4f3919adf003621e4c305a11dca80620871015a`):
//! - `widgets/dismissible.dart:19-22`: minimum fling velocity 700 px/s,
//!   minimum axis dominance 400 px/s, velocity scale 1/300 and default
//!   dismiss threshold 0.4.
//! - `widgets/dismissible.dart:341-359`: extent-to-direction mapping, with
//!   LTR/RTL handling on the horizontal axis, and per-direction threshold
//!   override.
//! - `widgets/dismissible.dart:468-498`: a fling is classified only when it
//!   is fast enough and dominant on the drag axis; its direction decides
//!   forward versus reverse.
//! - `widgets/dismissible.dart:527-535`: a non-fling completes only when the
//!   drag progress exceeds the threshold.
//!
//! These tests are a deterministic host projection and add no ABI or IR
//! field.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    StartToEnd,
    EndToStart,
    Up,
    Down,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlingKind {
    None,
    Forward,
    Reverse,
}

const MIN_FLING_VELOCITY: f64 = 700.0;
const MIN_FLING_DELTA: f64 = 400.0;
const DEFAULT_DISMISS_THRESHOLD: f64 = 0.4;

/// `dismissible.dart:341-355` for the vertical axis.
fn vertical_direction(extent: f64) -> Direction {
    if extent == 0.0 {
        Direction::None
    } else if extent > 0.0 {
        Direction::Down
    } else {
        Direction::Up
    }
}

/// `dismissible.dart:468-498` for the vertical axis.
fn describe_fling(extent: f64, vx: f64, vy: f64) -> FlingKind {
    if extent == 0.0 {
        return FlingKind::None;
    }
    if vy.abs() - vx.abs() < MIN_FLING_DELTA || vy.abs() < MIN_FLING_VELOCITY {
        return FlingKind::None;
    }
    let fling_direction = vertical_direction(vy);
    if fling_direction == vertical_direction(extent) {
        FlingKind::Forward
    } else {
        FlingKind::Reverse
    }
}

/// `dismissible.dart:527-535`: non-fling completion by progress.
fn settles_dismissed(progress: f64, threshold: f64) -> bool {
    progress > threshold
}

#[test]
fn default_threshold_is_zero_point_four() {
    assert_eq!(DEFAULT_DISMISS_THRESHOLD, 0.4);
}

#[test]
fn vertical_extent_maps_to_direction() {
    // dismissible.dart:354.
    assert_eq!(vertical_direction(1.0), Direction::Down);
    assert_eq!(vertical_direction(-1.0), Direction::Up);
    assert_eq!(vertical_direction(0.0), Direction::None);
}

#[test]
fn a_fling_needs_speed_and_axis_dominance() {
    // dismissible.dart:481-490.
    assert_eq!(describe_fling(1.0, 0.0, 800.0), FlingKind::Forward);
    assert_eq!(describe_fling(1.0, 0.0, 600.0), FlingKind::None);
    assert_eq!(describe_fling(1.0, 500.0, 800.0), FlingKind::None);
    assert_eq!(describe_fling(0.0, 0.0, 800.0), FlingKind::None);
}

#[test]
fn fling_direction_can_reverse_the_drag() {
    // dismissible.dart:494-497.
    assert_eq!(describe_fling(1.0, 0.0, -900.0), FlingKind::Reverse);
    assert_eq!(describe_fling(-1.0, 0.0, -900.0), FlingKind::Forward);
}

#[test]
fn non_fling_uses_the_threshold() {
    // dismissible.dart:527-535.
    assert!(settles_dismissed(0.5, DEFAULT_DISMISS_THRESHOLD));
    assert!(!settles_dismissed(0.3, DEFAULT_DISMISS_THRESHOLD));
    assert!(
        !settles_dismissed(0.5, 0.6),
        "a per-direction override wins"
    );
}

#[test]
fn dismiss_projection_is_deterministic() {
    let run = || {
        (
            vertical_direction(10.0),
            describe_fling(10.0, 0.0, 900.0),
            settles_dismissed(0.45, DEFAULT_DISMISS_THRESHOLD),
        )
    };
    assert_eq!(run(), run());
}
